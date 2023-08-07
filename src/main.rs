mod image;
mod menu;
mod video;
use std::sync::mpsc;
use std::time::Duration;

use kas::prelude::*;
use log::error;
use log::info;

use menu::Menu;

#[derive(Clone, Debug)]
pub enum GlobalMsg {
    TryLoadMovie,
}

#[derive(Debug)]
enum Msg {
    LoadMovieNone,
    LoadMovie(url::Url),
    LoadMovieFail,
    LoadMovieSuccess,
}

impl_scope! {
    #[widget{
        layout = column: [
            self.menu,
            self.image,
        ];
    }]
    #[derive(Debug)]
    struct Main {
        core: widget_core!(),
        #[widget] menu: Menu,
        #[widget] image: image::Image,
        n_images: usize,
        streamer: Option<video::Streamer>,
        msg_receiver: Option<mpsc::Receiver<video::VideoMessage>>,
        video_watcher_interval: Duration,
    }

    impl Self {
        fn new() -> Self {
            let mut img = image::Image::new("movie", 720, 480);
            let mut data = Vec::new();
            data.resize(720 * 480 * 4, 0);
            img.set_image(data, 720, 480);
            Self {
                core: Default::default(),
                menu: Menu::new(),
                image: img,
                n_images: 0,
                streamer: None,
                msg_receiver: None,
                video_watcher_interval: Duration::from_secs(1),
            }
        }
    }

    impl Widget for Self {
        fn handle_message(&mut self, mgr: &mut EventMgr) {
            //error!(">handle_message");
            if let Some(msg) = mgr.try_pop::<GlobalMsg>() {
                match msg {
                   GlobalMsg::TryLoadMovie => {
                        mgr.set_disabled(self.id(), true);
                        mgr.push_spawn(self.id(), try_load_movie());
                    }
                }
            }
            if let Some(msg) = mgr.try_pop::<Msg>() {
                match msg {
                    Msg::LoadMovieNone => {
                        mgr.set_disabled(self.id(), false);
                    }
                    Msg::LoadMovie(url) => {
                        mgr.set_disabled(self.id(), false);
                        error!("url is: {}", url);
                        //let (msg_sender, msg_receiver) = std::sync::mpsc::sync_channel(10);
                        error!("creating video...");
                        match video::Streamer::new(&url, true) {
                            Err(e) => {
                                error!("{:?}", e);
                                mgr.push(Msg::LoadMovieFail);
                            }
                            Ok(mut vs) => {
                                self.msg_receiver = vs.take_msg_receiver();
                                let fps = vs.framerate();
                                self.streamer = Some(vs);
                                self.n_images = 0;

                                //mgr.push_spawn(self.id(), video_message_handler("dummy".to_string(), msg_receiver.unwrap()));
                                self.video_watcher_interval = Duration::from_secs_f64(1.0f64 / (fps * 5.0f64));
                                mgr.request_update(self.id(), 3939, self.video_watcher_interval, true);
                                self.streamer.as_mut().unwrap().start();
                                mgr.push(Msg::LoadMovieSuccess);
                            }
                        }
                    }
                    Msg::LoadMovieFail => {
                        error!("load movie failed");
                    }
                    Msg::LoadMovieSuccess => {
                        info!("load movie successeded");
                    }
                }
            }
            //error!("<handle_message");
        }
        fn handle_event(&mut self, mgr: &mut EventMgr, ev: Event) -> Response {
            match ev {
                Event::TimerUpdate(3939) => {
                    let mut new_sample: Option<(Vec<u8>, u32, u32)> = None;
                    if let Some(ref mut msg_receiver) = self.msg_receiver {
                        for msg in msg_receiver.try_iter() {
                            match msg {
                                video::VideoMessage::NewSample(data, width, height) => {
                                    self.n_images += 1;
                                    new_sample = Some((data, width, height));
                                    //error!("[{}] new sample", self.n_images);
                                }
                                video::VideoMessage::GstMessage(gst_msg) => {
                                    match gst_msg.view() {
                                        gstreamer::MessageView::Eos(..) => {
                                            error!("[{}] eos", self.n_images);
                                            //return Msg::VideoFin;
                                            return Response::Used;
                                        }
                                        gstreamer::MessageView::Error(err) => {
                                            error!("[{}] Error: {} ({:?})", self.n_images, err.error(), err.debug());
                                            //return Msg::VideoFin;
                                            return Response::Used;
                                        }
                                        _ev => {
                                            //error!("unknown event: {:?}", ev);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if let Some((data, width, height)) = new_sample {
                        if let  Some(a) = self.image.set_image(data, width, height) {
                            *mgr |= a;
                        }
                    }
                    let _ = mgr.request_update(self.id(), 3939, self.video_watcher_interval, true);
                    Response::Used
                }
                _ => {
                    Response::Unused
                }
            }
        }
    }
    impl Window for Self {
        fn title(&self) -> &str { "my f2f" }
    }
}

async fn try_load_movie() -> Msg {
    // todo mutex
    let file = rfd::AsyncFileDialog::new()
        .add_filter("movie", &["mp4", "mkv", "avi"])
        .set_directory("/")
        .pick_file()
        .await;
    match file {
        None => Msg::LoadMovieNone,
        Some(f) => {
            let path = f.path().to_string_lossy();
            let url =
                url::Url::parse(&format!("file://{}", path.replace(":", "/"))).expect("parse url");
            Msg::LoadMovie(url)
        }
    }
}

#[tokio::main]
async fn main() -> kas::shell::Result<()> {
    env_logger::init();
    let theme = kas::theme::SimpleTheme::new().with_font_size(24.0);
    let shell = kas::shell::DefaultShell::new(theme)?;
    let main = Main::new();
    shell.with(main)?.run();
}
