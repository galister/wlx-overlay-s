use std::{
    sync::{Arc, LazyLock, atomic::AtomicU64},
    time::Instant,
};

use glam::{Affine2, Vec2, vec2};
use vulkano::image::view::ImageView;
use wlx_capture::{WlxCapture, frame::Transform};

use crate::{
    backend::input::{Haptics, PointerHit, PointerMode},
    graphics::{CommandBuffers, ExtentExt},
    state::AppState,
    subsystem::hid::{MOUSE_LEFT, MOUSE_MIDDLE, MOUSE_RIGHT},
    windowing::backend::{FrameMeta, OverlayBackend, ShouldRender},
};

use super::capture::{ScreenPipeline, WlxCaptureIn, WlxCaptureOut, receive_callback};

const CURSOR_SIZE: f32 = 16. / 1440.;

static START: LazyLock<Instant> = LazyLock::new(Instant::now);
static NEXT_MOVE: AtomicU64 = AtomicU64::new(0);

fn can_move() -> bool {
    START.elapsed().as_millis() as u64 > NEXT_MOVE.load(std::sync::atomic::Ordering::Relaxed)
}

fn set_next_move(millis_from_now: u64) {
    NEXT_MOVE.store(
        START.elapsed().as_millis() as u64 + millis_from_now,
        std::sync::atomic::Ordering::Relaxed,
    );
}

pub struct ScreenBackend {
    name: Arc<str>,
    capture: Box<dyn WlxCapture<WlxCaptureIn, WlxCaptureOut>>,
    pipeline: Option<ScreenPipeline>,
    cur_frame: Option<WlxCaptureOut>,
    meta: Option<FrameMeta>,
    mouse_transform: Affine2,
    interaction_transform: Option<Affine2>,
}

impl ScreenBackend {
    pub fn new_raw(
        name: Arc<str>,
        capture: Box<dyn WlxCapture<WlxCaptureIn, WlxCaptureOut>>,
    ) -> Self {
        Self {
            name,
            capture,
            pipeline: None,
            cur_frame: None,
            meta: None,
            mouse_transform: Affine2::ZERO,
            interaction_transform: None,
        }
    }

    pub(super) fn set_mouse_transform(&mut self, pos: Vec2, size: Vec2, transform: Transform) {
        self.mouse_transform = match transform {
            Transform::Rotated90 | Transform::Flipped90 => Affine2::from_cols(
                vec2(0., size.y),
                vec2(-size.x, 0.),
                vec2(pos.x + size.x, pos.y),
            ),
            Transform::Rotated180 | Transform::Flipped180 => Affine2::from_cols(
                vec2(-size.x, 0.),
                vec2(0., -size.y),
                vec2(pos.x + size.x, pos.y + size.y),
            ),
            Transform::Rotated270 | Transform::Flipped270 => Affine2::from_cols(
                vec2(0., -size.y),
                vec2(size.x, 0.),
                vec2(pos.x, pos.y + size.y),
            ),
            _ => Affine2::from_cols(vec2(size.x, 0.), vec2(0., size.y), pos),
        };
    }

    pub(super) fn set_interaction_transform(&mut self, res: Vec2, transform: Transform) {
        let center = Vec2 { x: 0.5, y: 0.5 };
        self.interaction_transform = Some(match transform {
            Transform::Rotated90 | Transform::Flipped90 => {
                Affine2::from_cols(Vec2::NEG_Y * (res.x / res.y), Vec2::NEG_X, center)
            }
            Transform::Rotated180 | Transform::Flipped180 => {
                Affine2::from_cols(Vec2::NEG_X, Vec2::NEG_Y * (-res.x / res.y), center)
            }
            Transform::Rotated270 | Transform::Flipped270 => {
                Affine2::from_cols(Vec2::Y * (res.x / res.y), Vec2::X, center)
            }
            _ if res.y > res.x => {
                // Xorg upright screens
                Affine2::from_cols(Vec2::X * (res.y / res.x), Vec2::NEG_Y, center)
            }
            _ => Affine2::from_cols(Vec2::X, Vec2::NEG_Y * (res.x / res.y), center),
        });
    }
}

impl OverlayBackend for ScreenBackend {
    fn init(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        Ok(())
    }
    fn should_render(&mut self, app: &mut AppState) -> anyhow::Result<ShouldRender> {
        if !self.capture.is_ready() {
            let supports_dmabuf = app
                .gfx
                .device
                .enabled_extensions()
                .ext_external_memory_dma_buf
                && self.capture.supports_dmbuf();

            let allow_dmabuf = &*app.session.config.capture_method != "pw_fallback"
                && &*app.session.config.capture_method != "screencopy";

            let capture_method = app.session.config.capture_method.clone();

            let dmabuf_formats = if !supports_dmabuf {
                log::info!("Capture method does not support DMA-buf");
                if app.gfx_extras.queue_capture.is_none() {
                    log::warn!(
                        "Current GPU does not support multiple queues. Software capture will take place on the main thread. Expect degraded performance."
                    );
                }
                &Vec::new()
            } else if !allow_dmabuf {
                log::info!("Not using DMA-buf capture due to {capture_method}");
                if app.gfx_extras.queue_capture.is_none() {
                    log::warn!(
                        "Current GPU does not support multiple queues. Software capture will take place on the main thread. Expect degraded performance."
                    );
                }
                &Vec::new()
            } else {
                log::warn!(
                    "Using DMA-buf capture. If screens are blank for you, switch to SHM using:"
                );
                log::warn!(
                    "echo 'capture_method: pw_fallback' > ~/.config/wlxoverlay/conf.d/pw_fallback.yaml"
                );

                &app.gfx_extras.drm_formats
            };

            let user_data = WlxCaptureIn::new(self.name.clone(), app);
            self.capture
                .init(dmabuf_formats, user_data, receive_callback);
            self.capture.request_new_frame();
            return Ok(ShouldRender::Unable);
        }

        if let Some(frame) = self.capture.receive() {
            let meta = frame.get_frame_meta(&app.session.config);

            if let Some(pipeline) = self.pipeline.as_mut() {
                if self.meta.is_some_and(|old| old.extent != meta.extent) {
                    pipeline.set_extent(app, [meta.extent[0] as _, meta.extent[1] as _])?;
                    self.set_interaction_transform(
                        meta.extent.extent_vec2(),
                        frame.get_transform(),
                    );
                }
            } else {
                self.pipeline = Some(ScreenPipeline::new(&meta, app)?);
                self.set_interaction_transform(meta.extent.extent_vec2(), frame.get_transform());
            }

            self.meta = Some(meta);
            self.cur_frame = Some(frame);
        }

        if self.cur_frame.is_some() {
            Ok(ShouldRender::Should)
        } else {
            Ok(ShouldRender::Unable)
        }
    }
    fn render(
        &mut self,
        app: &mut AppState,
        tgt: Arc<ImageView>,
        buf: &mut CommandBuffers,
        alpha: f32,
    ) -> anyhow::Result<bool> {
        let Some(capture) = self.cur_frame.take() else {
            return Ok(false);
        };

        // want panic; must be Some if cur_frame is also Some
        self.pipeline
            .as_mut()
            .unwrap()
            .render(&capture, app, tgt, buf, alpha)?;

        self.capture.request_new_frame();
        Ok(true)
    }
    fn pause(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        self.capture.pause();
        Ok(())
    }
    fn resume(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        self.capture.resume();
        Ok(())
    }
    fn frame_meta(&mut self) -> Option<FrameMeta> {
        self.meta
    }

    fn on_hover(&mut self, app: &mut AppState, hit: &PointerHit) -> Option<Haptics> {
        #[cfg(debug_assertions)]
        log::trace!("Hover: {:?}", hit.uv);
        if can_move()
            && (!app.session.config.focus_follows_mouse_mode
                || app.input_state.pointers[hit.pointer].now.move_mouse)
        {
            let pos = self.mouse_transform.transform_point2(hit.uv);
            app.hid_provider.inner.mouse_move(pos);
            set_next_move(u64::from(app.session.config.mouse_move_interval_ms));
        }
        None
    }
    fn on_pointer(&mut self, app: &mut AppState, hit: &PointerHit, pressed: bool) {
        let btn = match hit.mode {
            PointerMode::Right => MOUSE_RIGHT,
            PointerMode::Middle => MOUSE_MIDDLE,
            _ => MOUSE_LEFT,
        };

        if pressed {
            set_next_move(u64::from(app.session.config.click_freeze_time_ms));
        }

        app.hid_provider.inner.send_button(btn, pressed);

        if !pressed {
            return;
        }
        let pos = self.mouse_transform.transform_point2(hit.uv);
        app.hid_provider.inner.mouse_move(pos);
    }
    fn on_scroll(&mut self, app: &mut AppState, _hit: &PointerHit, delta_y: f32, delta_x: f32) {
        app.hid_provider
            .inner
            .wheel((delta_y * 64.) as i32, (delta_x * 64.) as i32);
    }
    fn on_left(&mut self, _app: &mut AppState, _hand: usize) {}

    fn get_interaction_transform(&mut self) -> Option<Affine2> {
        self.interaction_transform
    }
}
