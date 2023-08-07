use kas::prelude::*;
use kas::resvg::{tiny_skia, tiny_skia::Pixmap, Canvas, CanvasProgram};
//use log::{error, info};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex,
};

#[derive(Debug, Clone)]
struct ImageProgramData {
    pixmap: Option<Pixmap>,
}
impl ImageProgramData {
    pub fn new(name: &str) -> (ImageProgramDrawer, ImageProgramSetter) {
        let data = Self { pixmap: None };
        let arc = Arc::new((Mutex::new(data), Condvar::new()));
        let need_redraw = Arc::new(AtomicBool::new(true));
        (
            ImageProgramDrawer {
                _name: name.to_string(),
                arc: arc.clone(),
                need_redraw: need_redraw.clone(),
            },
            ImageProgramSetter {
                _name: name.to_string(),
                arc: arc.clone(),
                need_redraw: need_redraw.clone(),
            },
        )
    }
}

#[derive(Debug, Clone)]
struct ImageProgramSetter {
    _name: String,
    arc: Arc<(Mutex<ImageProgramData>, Condvar)>,
    need_redraw: Arc<AtomicBool>,
}
impl ImageProgramSetter {
    pub fn set_image(&mut self, rgba: Vec<u8>, width: u32, height: u32) -> Option<Action> {
        if rgba.len() == 0 {
            return None;
        }
        let size = tiny_skia_path::IntSize::from_wh(width, height).expect("IntSize::from_wh");
        let pixmap = Pixmap::from_vec(rgba, size).expect("Pixmap::from_vec");

        let (lock, cvar) = &*self.arc;
        let mut guard = lock.lock().unwrap();
        let data = &mut *guard;
        {
            data.pixmap = Some(pixmap);
            cvar.notify_one();
        }

        self.need_redraw.store(true, Ordering::Relaxed);
        Some(Action::REDRAW)
    }
}

#[derive(Debug, Clone)]
struct ImageProgramDrawer {
    _name: String,
    arc: Arc<(Mutex<ImageProgramData>, Condvar)>,
    need_redraw: Arc<AtomicBool>,
}
impl ImageProgramDrawer {
    pub fn need_redraw(&mut self) -> bool {
        self.need_redraw.load(Ordering::Relaxed)
    }
    pub fn draw(&mut self, target: &mut Pixmap) {
        //error!("[{}] > drawer draw", self.name);

        let (lock, cvar) = &*self.arc;
        let mut guard = lock.lock().unwrap();
        while guard.pixmap.is_none() {
            guard = cvar.wait(guard).unwrap();
            //error!("[{}] wake", self.name);
        }
        if let Some(pixmap) = guard.pixmap.take() {
            let paint = tiny_skia::PixmapPaint {
                opacity: 1.0f32,
                blend_mode: tiny_skia::BlendMode::Source,
                quality: tiny_skia::FilterQuality::Nearest,
            };
            let tr = tiny_skia::Transform::identity();
            target.draw_pixmap(0, 0, pixmap.as_ref(), &paint, tr, None);
        }
        //error!("[{}] < drawer draw", self.name);
    }
}

#[derive(Debug, Clone)]
pub struct ImageProgram {
    _name: String,
    drawer: ImageProgramDrawer,
    setter: Option<ImageProgramSetter>,
}
impl Default for ImageProgram {
    fn default() -> Self {
        Self::new("default")
    }
}
impl ImageProgram {
    fn new(name: &str) -> Self {
        let (drawer, setter) = ImageProgramData::new(name);
        Self {
            _name: name.to_string(),
            drawer,
            setter: Some(setter),
        }
    }
    fn new_and_take_setter(name: &str) -> (Self, ImageProgramSetter) {
        let mut pg = Self::new(name);
        let setter = pg.take_setter();
        (pg, setter)
    }
    fn take_setter(&mut self) -> ImageProgramSetter {
        self.setter.take().unwrap()
    }
}
impl CanvasProgram for ImageProgram {
    fn need_redraw(&mut self) -> bool {
        //error!("[{}] need redraw: {}", self.name, self.first);
        self.drawer.need_redraw()
    }
    fn draw(&mut self, pixmap: &mut Pixmap) {
        //error!("[{}] pg draw", self.name);
        self.drawer.draw(pixmap);
    }
}

impl_scope! {
    #[widget{
        layout = column: [ align(center): self.canvas ];
    }]
    #[derive(Clone, Debug)]
    pub struct Image {
        core: widget_core!(),
        _name: String,
        image_setter: ImageProgramSetter,
        #[widget] canvas: Canvas<ImageProgram>,
    }

    impl Self {
        pub fn new(name: &str, width: u32, height: u32) -> Self {
            let (pg, setter) = ImageProgram::new_and_take_setter(name);
            let size = kas::layout::LogicalSize::try_conv((width, height)).unwrap();
            let canvas = Canvas::new(pg).with_size(size);
            Self {
                core: Default::default(),
                _name: name.to_string(),
                image_setter: setter,
                canvas,
            }
        }

        pub fn set_image(&mut self, rgba: Vec<u8>, width: u32, height: u32) -> Option<Action> {
            return self.image_setter.set_image(rgba, width, height);
        }
    }
}
