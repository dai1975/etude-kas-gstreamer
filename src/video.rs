use gst::prelude::*;
use gstreamer as gst;
use gstreamer_app as gst_app;
use log::{error, info};
use std::sync::{mpsc, Arc, Mutex};

#[derive(Debug, Clone)]
pub enum VideoMessage {
    GstMessage(gst::Message),
    NewSample,
}

#[derive(Default, Debug)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

/// Video player which handles multimedia playback.
pub struct Streamer {
    source: gst::Bin,
    frame: Arc<Mutex<Frame>>,
    width: u32,
    height: u32,
    framerate: gst::Fraction,
    duration: std::time::Duration,
}

impl Drop for Streamer {
    fn drop(&mut self) {
        info!("Streamer#drop");
        self.source.set_state(gst::State::Null).unwrap();
    }
}

impl std::fmt::Debug for Streamer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "video::Streamer")
    }
}

impl Streamer {
    pub fn new(uri: &url::Url) -> (Self, mpsc::Receiver<VideoMessage>) {
        gst::init().expect("gst.init");
        let (msg_sender, msg_receiver) = std::sync::mpsc::sync_channel::<VideoMessage>(10);
        let msg_sender = Arc::new(Mutex::new(msg_sender));
        let frame = Arc::new(Mutex::new(Frame::default()));

        let source = gst::parse_launch(&format!("playbin uri=\"{}\" video-sink=\"videoconvert ! videoscale ! appsink name=app_sink caps=video/x-raw,format=RGBA,pixel-aspect-ratio=1/1\"", uri.as_str())).unwrap();

        let video_sink: gst::Element = source.property::<gst::Element>("video-sink");
        let pad = video_sink.pads().get(0).cloned().unwrap();
        let pad = pad.dynamic_cast::<gst::GhostPad>().unwrap();
        let bin = pad
            .parent_element()
            .unwrap()
            .downcast::<gst::Bin>()
            .unwrap();

        let app_sink = bin.by_name("app_sink").unwrap();
        let app_sink = app_sink.downcast::<gst_app::AppSink>().unwrap();

        let frame_ref = frame.clone();
        let msg_sender_ref = msg_sender.clone();
        app_sink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;
                    let map = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;

                    let pad = sink.static_pad("sink").ok_or(gst::FlowError::Error)?;

                    let caps = pad.current_caps().ok_or(gst::FlowError::Error)?;
                    let s = caps.structure(0).ok_or(gst::FlowError::Error)?;
                    let width = s.get::<i32>("width").map_err(|_| gst::FlowError::Error)?;
                    let height = s.get::<i32>("height").map_err(|_| gst::FlowError::Error)?;
                    let width = u32::try_from(width).map_err(|_| gst::FlowError::Error)?;
                    let height = u32::try_from(height).map_err(|_| gst::FlowError::Error)?;

                    match frame_ref.lock() {
                        Err(_) => Err(gst::FlowError::Error),
                        Ok(mut f) => {
                            let datasize: usize = (width * height * 4).try_into().unwrap();
                            f.width = width;
                            f.height = height;
                            if f.data.len() < datasize {
                                f.data.resize(datasize, 0);
                            }
                            unsafe {
                                std::ptr::copy_nonoverlapping(
                                    map.as_slice().as_ptr(),
                                    f.data.as_mut_ptr(),
                                    datasize,
                                );
                            }
                            Ok(())
                        }
                    }?;

                    msg_sender_ref
                        .lock()
                        .unwrap()
                        .send(VideoMessage::NewSample)
                        .map_err(|_| gst::FlowError::Error)?;
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );

        source.set_state(gst::State::Playing).unwrap();

        source.state(gst::ClockTime::from_seconds(5)).0.unwrap();
        source.set_state(gst::State::Paused).unwrap();

        let msg_sender_ref = msg_sender.clone();
        source
            .bus()
            .unwrap()
            .add_watch(move |bus, msg| {
                if let Ok(_) = msg_sender_ref
                    .lock()
                    .unwrap()
                    .send(VideoMessage::GstMessage(msg.clone()))
                {
                    bus.pop();
                }
                gst::glib::source::Continue(true)
            })
            .unwrap();

        let caps = pad.current_caps().unwrap();
        let s = caps.structure(0).unwrap();
        let width = s.get::<i32>("width").unwrap();
        let height = s.get::<i32>("height").unwrap();
        let width = u32::try_from(width).unwrap();
        let height = u32::try_from(height).unwrap();
        let framerate = s.get::<gst::Fraction>("framerate").unwrap();

        let duration = std::time::Duration::from_nanos(
            source
                .query_duration::<gst::ClockTime>()
                .unwrap()
                .nseconds(),
        );
        info!(
            "width={width}, height={height}, framerate={framerate}, duration={}",
            duration.as_secs_f32()
        );

        let streamer = Streamer {
            source: source.downcast::<gst::Bin>().unwrap(),
            frame,

            width,
            height,
            framerate,
            duration,
        };
        (streamer, msg_receiver)
    }

    pub fn frame(&mut self) -> Arc<Mutex<Frame>> {
        self.frame.clone()
    }

    pub fn start(&mut self) {
        let main_loop = gst::glib::MainLoop::new(None, false);
        self.source
            .seek_simple(gst::SeekFlags::FLUSH, gst::ClockTime::from_seconds(0))
            .unwrap();
        self.source.set_state(gst::State::Playing).unwrap();
        std::thread::spawn(move || {
            error!("mailloop start");
            main_loop.run();
            error!("mailloop end");
        });
    }

    #[inline(always)]
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    #[inline(always)]
    pub fn framerate(&self) -> gst::Fraction {
        self.framerate
    }

    #[inline(always)]
    pub fn duration(&self) -> std::time::Duration {
        self.duration
    }
}
