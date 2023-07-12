// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License in the LICENSE-APACHE file or at:
//     https://www.apache.org/licenses/LICENSE-2.0

//! 2D pixmap widget

use crate::video;
use kas::draw::ImageHandle;
use kas::layout::PixmapScaling;
use kas::prelude::*;
use std::sync::{Arc, Mutex};
//use kas_widgets::Image as KasImage;
//use log::{error, info};

impl_scope! {
    #[derive(Clone, Debug, Default)]
    #[widget]
    pub struct Image {
        core: widget_core!(),
        scaling: PixmapScaling,
        handle: Option<ImageHandle>,
        streamer: Option<Arc<Mutex<video::Frame>>>,
    }

    impl Self {
        pub fn new(width: f32, height: f32) -> Self {
            let mut r = Self::default();
            r.scaling.size = kas::layout::LogicalSize(width, height);
            r
        }

        pub fn set_streamer(&mut self, streamer: Option<Arc<Mutex<video::Frame>>>, size: (u32, u32)) -> Option<Action> {
            self.streamer = streamer;
            if self.streamer.is_some() {
                // TODO: Resizing is not called. Why?
                self.scaling.size = kas::layout::LogicalSize::try_conv(size).unwrap();
                self.scaling.fix_aspect = true;
                Some(Action::RESIZE)
            } else {
                None
            }
        }
    }

    impl Layout for Image {
        fn size_rules(&mut self, size_mgr: SizeMgr, axis: AxisInfo) -> SizeRules {
            self.scaling.size_rules(size_mgr, axis)
        }

        fn set_rect(&mut self, mgr: &mut ConfigMgr, rect: Rect) {
            let scale_factor = mgr.size_mgr().scale_factor();
            self.core.rect = self.scaling.align_rect(rect, scale_factor);
        }

        fn draw(&mut self, mut draw: DrawMgr) {
            if let Some(ref frame) = self.streamer {
                if let Ok(f) = frame.lock() {
                    let frame_size = kas::geom::Size::conv((f.width, f.height));
                    let ds = draw.draw_shared();
                    if let Some(image_size) = self.handle.as_ref().and_then(|ih| ds.image_size(ih)) {
                        if image_size != frame_size {
                            if let Some(ih) = self.handle.take() {
                                ds.image_free(ih);
                            }
                        }
                    }
                    if self.handle.is_none() {
                        self.handle = ds.image_alloc((f.width, f.height)).ok();
                        self.scaling.size = kas::layout::LogicalSize::try_conv((f.width, f.height)).unwrap();
                    }
                    if let Some(ih) = &self.handle {
                        ds.image_upload(ih, &f.data, kas::draw::ImageFormat::Rgba8);
                    }
               }
            }
            if let Some(id) = self.handle.as_ref().map(|h| h.id()) {
                draw.image(self.rect(), id);
            }
         }
    }
}
