use gst::prelude::*;
use gstreamer as gst;
use gstreamer_app as gst_app;
//use iced::{image as img, Command, Image, Subscription};
use log::{error, info};
use num_traits::ToPrimitive;
use std::sync::mpsc;
//use std::time::Duration;
use thiserror::Error;

/// Position in the media.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Position {
    /// Position based on time.
    ///
    /// Not the most accurate format for videos.
    Time(std::time::Duration),
    /// Position based on nth frame.
    Frame(u64),
}

impl From<Position> for gst::GenericFormattedValue {
    fn from(pos: Position) -> Self {
        match pos {
            Position::Time(t) => gst::ClockTime::from_nseconds(t.as_nanos() as _).into(),
            Position::Frame(f) => gst::format::Default::from_u64(f).into(),
        }
    }
}

impl From<std::time::Duration> for Position {
    fn from(t: std::time::Duration) -> Self {
        Position::Time(t)
    }
}

impl From<u64> for Position {
    fn from(f: u64) -> Self {
        Position::Frame(f)
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Glib(#[from] gst::glib::Error),
    #[error("{0}")]
    Bool(#[from] gst::glib::BoolError),
    #[error("{0}")]
    StateChange(#[from] gst::StateChangeError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("failed to get media capabilities")]
    Caps,
    #[error("failed to query media duration or position")]
    Duration,
}

#[derive(Debug)]
pub enum VideoMessage {
    GstMessage(gst::Message),
    NewSample(Vec<u8>, u32, u32),
}

#[derive(Default, Debug)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

/// Video player which handles multimedia playback.
pub struct Streamer {
    do_sync: bool,
    pipeline: gst::Bin,
    app_sink: gst_app::AppSink,
    width: u32,
    height: u32,
    framerate: f64,
    duration: std::time::Duration,
    msg_sender: mpsc::SyncSender<VideoMessage>,
    msg_receiver: Option<mpsc::Receiver<VideoMessage>>,
}

impl Drop for Streamer {
    fn drop(&mut self) {
        error!("Streamer#drop");
        self.pipeline
            .set_state(gst::State::Null)
            .expect("failed to set state");
    }
}

impl std::fmt::Debug for Streamer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "video::Streamer")
    }
}

impl Streamer {
    pub fn new(uri: &url::Url, sync: bool) -> Result<Self, Error> {
        gst::init()?;
        let (msg_sender, msg_receiver) = std::sync::mpsc::sync_channel::<VideoMessage>(10);

        let pipeline = gst::parse_launch(&format!("playbin uri=\"{uri}\" video-sink=\"videoconvert ! videoscale ! appsink name=app_sink caps=video/x-raw,format=RGBA,pixel-aspect-ratio=1/1\""))?;

        let video_sink: gst::Element = pipeline.property::<gst::Element>("video-sink");
        let bin = video_sink.downcast::<gst::Bin>().unwrap();
        let app_sink = bin
            .by_name("app_sink")
            .unwrap()
            .downcast::<gst_app::AppSink>()
            .unwrap();

        app_sink.set_sync(false);
        pipeline.set_state(gst::State::Playing)?;
        // wait for up to 5 seconds until the decoder gets the source capabilities
        pipeline.state(gst::ClockTime::from_seconds(5)).0?;
        pipeline.set_state(gst::State::Paused)?;

        // extract resolution and framerate
        let pads = app_sink.sink_pads();
        let pad = pads.get(0).unwrap();

        let caps = pad.current_caps().ok_or(Error::Caps)?;
        let s = caps.structure(0).ok_or(Error::Caps)?;
        let width = s.get::<i32>("width").map_err(|_| Error::Caps)?;
        let height = s.get::<i32>("height").map_err(|_| Error::Caps)?;
        let width = u32::try_from(width).map_err(|_| Error::Caps)?;
        let height = u32::try_from(height).map_err(|_| Error::Caps)?;
        info!("width={width}, height={height}");
        let framerate = s
            .get::<gst::Fraction>("framerate")
            .map_err(|_| Error::Caps)?;
        info!("framerate={framerate}");

        let duration = std::time::Duration::from_nanos(
            pipeline
                .query_duration::<gst::ClockTime>()
                .ok_or(Error::Duration)?
                .nseconds(),
        );
        info!("duration={:?}", duration);

        Ok(Streamer {
            do_sync: sync,
            pipeline: pipeline.downcast::<gst::Bin>().unwrap(),
            app_sink: app_sink,
            msg_sender: msg_sender,
            msg_receiver: Some(msg_receiver),
            width,
            height,
            framerate: num_rational::Rational32::new(
                    framerate.numer() as _,
                    framerate.denom() as _,
                )
                .to_f64().unwrap(/* if the video framerate is bad then it would've been implicitly caught far earlier */),
            duration,
        })
    }

    pub fn take_msg_receiver(&mut self) -> Option<mpsc::Receiver<VideoMessage>> {
        self.msg_receiver.take()
    }
    pub fn start(&mut self) {
        //let ctx = gst::glib::MainContext::default();
        //let main_loop = gst::glib::MainLoop::new(Some(&ctx), false);
        let main_loop = gst::glib::MainLoop::new(None, false);

        if true {
            let msg_sender_sink = self.msg_sender.clone();
            let main_loop_ref = main_loop.clone();
            let bus = self.pipeline.bus().unwrap();
            let _ = bus.remove_watch();
            let _ = bus.add_watch(move |_bus, msg| match msg.view() {
                gst::MessageView::Eos(_) => {
                    error!("gst.Eos");
                    let _ = msg_sender_sink.send(VideoMessage::GstMessage(msg.clone()));
                    main_loop_ref.quit();
                    gst::glib::source::Continue(false)
                }
                gst::MessageView::Error(_) => {
                    error!("gst.Error");
                    let _ = msg_sender_sink.send(VideoMessage::GstMessage(msg.clone()));
                    main_loop_ref.quit();
                    gst::glib::source::Continue(false)
                }
                gst::MessageView::StateChanged(..) => gst::glib::source::Continue(true),
                gst::MessageView::Tag(..) => gst::glib::source::Continue(true),
                m => {
                    error!("gst.other: {:?}", m);
                    gst::glib::source::Continue(true)
                }
            });
        }
        if true {
            let msg_sender_sink = self.msg_sender.clone();
            self.app_sink.set_callbacks(
                gst_app::AppSinkCallbacks::builder()
                    .new_sample(move |sink| {
                        let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?; // it fires eos event

                        let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;
                        let bufmap = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;

                        let pad = sink.static_pad("sink").ok_or(gst::FlowError::Error)?;

                        let caps = pad.current_caps().ok_or(gst::FlowError::Error)?;
                        let s = caps.structure(0).ok_or(gst::FlowError::Error)?;
                        let width = s.get::<i32>("width").map_err(|_| gst::FlowError::Error)?;
                        let height = s.get::<i32>("height").map_err(|_| gst::FlowError::Error)?;
                        let width = u32::try_from(width).map_err(|_| gst::FlowError::Error)?;
                        let height = u32::try_from(height).map_err(|_| gst::FlowError::Error)?;

                        let senddata = bufmap.to_vec();
                        let _ =
                            msg_sender_sink.send(VideoMessage::NewSample(senddata, width, height));
                        Ok(gst::FlowSuccess::Ok)
                    })
                    .build(),
            );
        }

        let _ = self.pipeline.seek(
            1.0f64,
            gst::SeekFlags::FLUSH,
            gst::SeekType::Set,
            gst::format::Bytes::ZERO,
            gst::SeekType::End,
            gst::format::Bytes::ZERO,
        );
        self.app_sink.set_sync(self.do_sync);
        self.pipeline.set_state(gst::State::Playing).unwrap();
        std::thread::spawn(move || {
            error!("mailloop start");
            main_loop.run();
            error!("mailloop end");
        });
    }

    #[inline(always)]
    #[allow(dead_code)]
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    #[inline(always)]
    #[allow(dead_code)]
    pub fn framerate(&self) -> f64 {
        self.framerate
    }

    #[inline(always)]
    #[allow(dead_code)]
    pub fn duration(&self) -> std::time::Duration {
        self.duration
    }
}
