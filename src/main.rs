mod image;
mod menu;
mod video;
use kas::prelude::*;
use log::error;
use log::info;
use std::time::Duration;

use menu::Menu;

#[derive(Clone, Debug)]
pub enum GlobalMsg {
    TryLoadMovie,
}

#[derive(Clone, Debug)]
enum Msg {
    LoadMovieNone,
    LoadMovie(url::Url),
    LoadMovieFail,
    LoadMovieSuccess,
    PlayMovieEnd,
    PlayMovieError,
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
        streamer: Option<video::Streamer>,
    }

    impl Self {
        fn new() -> Self {
            let image = image::Image::new(1080f32, 720f32);
            Self {
                core: Default::default(),
                menu: Menu::new(),
                image: image,
                streamer: None,
            }
        }
        fn start_movie(&mut self, mgr: &mut EventMgr, url: url::Url) {
            let (mut vs, msg_receiver) = video::Streamer::new(&url); //LoadMovieFail
            let frame = vs.frame().clone();
            let size = vs.size();
            self.image.set_streamer(Some(frame), size).map(|a| *mgr |= a);
            mgr.redraw(self.image.id());
            mgr.request_update(self.image.id(), 0, std::time::Duration::new(0, 1), true);
            vs.start();
            self.streamer = Some(vs);
            mgr.push(Msg::LoadMovieSuccess);
            mgr.push_spawn(self.id(), async move {
                for video_msg in msg_receiver.iter() {
                    match video_msg {
                        video::VideoMessage::NewSample => {
                        }
                        video::VideoMessage::GstMessage(gst_msg) => {
                            match gst_msg.view() {
                                gstreamer::MessageView::Eos(..) => {
                                    return Msg::PlayMovieEnd;
                                }
                                gstreamer::MessageView::Error(err) => {
                                    error!("Error: {} ({:?})", err.error(), err.debug());
                                    return Msg::PlayMovieError;
                                }
                                _ => (),
                            }
                        }
                    }
                }
                return Msg::PlayMovieError;
            });
        }
        fn timer_updated(&mut self, mgr: &mut EventMgr) {
            if self.streamer.is_some() {
                mgr.redraw(self.image.id());
                mgr.request_update(self.id(), 0, Duration::new(0, 1), true);
            }
        }
    }
    impl Widget for Self {
        fn handle_event(&mut self, mgr: &mut EventMgr, event: Event) -> Response {
            match event {
                Event::TimerUpdate(0) => {
                    self.timer_updated(mgr);
                    Response::Used
                }
                _ => Response::Unused,
            }
        }
        fn handle_message(&mut self, mgr: &mut EventMgr) {
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
                        mgr.set_disabled(self.id(), true);
                    }
                    Msg::LoadMovie(url) => {
                        self.start_movie(mgr, url);
                    }
                    Msg::LoadMovieFail => {
                        error!("load movie failed");
                        mgr.set_disabled(self.id(), false);
                    }
                    Msg::LoadMovieSuccess => {
                        info!("load movie successeded");
                        mgr.set_disabled(self.id(), false);
                    }
                    Msg::PlayMovieEnd => {
                        self.streamer = None;
                    }
                    Msg::PlayMovieError => {
                        error!("PlayMovieError");
                        self.streamer = None;
                    }
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
    let main = Main::new();
    kas::shell::DefaultShell::new(theme)?.with(main)?.run();
}
