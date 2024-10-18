use glam::{vec3a, Affine2};
use std::{cell::RefCell, rc::Rc, sync::Arc};
use vulkano::image::SubresourceLayout;
use wlx_capture::frame::{DmabufFrame, FourCC, FrameFormat, FramePlane};

use crate::{
    backend::{
        input::{self, InteractionHandler},
        overlay::{ui_transform, OverlayData, OverlayRenderer, OverlayState, SplitOverlayBackend},
        wayvr,
    },
    graphics::WlxGraphics,
    state::{self, KeyboardFocus},
};

pub struct WayVRContext {
    wayvr: Rc<RefCell<wayvr::WayVR>>,
    display: wayvr::display::DisplayHandle,
    width: u32,
    height: u32,
}

#[derive(Default)]
pub struct WayVRProcess<'a> {
    pub exec_path: &'a str,
    pub args: &'a [&'a str],
    pub env: &'a [(&'a str, &'a str)],
}

impl WayVRContext {
    pub fn new(
        wvr: Rc<RefCell<wayvr::WayVR>>,
        width: u32,
        height: u32,
        processes: &[WayVRProcess],
    ) -> anyhow::Result<Self> {
        let mut wayvr = wvr.borrow_mut();

        let display = wayvr.create_display(width, height)?;

        for process in processes {
            wayvr.spawn_process(display, process.exec_path, process.args, process.env)?;
        }

        Ok(Self {
            wayvr: wvr.clone(),
            display,
            width,
            height,
        })
    }
}

pub struct WayVRInteractionHandler {
    context: Rc<RefCell<WayVRContext>>,
    mouse_transform: Affine2,
}

impl WayVRInteractionHandler {
    pub fn new(context: Rc<RefCell<WayVRContext>>, mouse_transform: Affine2) -> Self {
        Self {
            context,
            mouse_transform,
        }
    }
}

impl InteractionHandler for WayVRInteractionHandler {
    fn on_hover(
        &mut self,
        _app: &mut state::AppState,
        hit: &input::PointerHit,
    ) -> Option<input::Haptics> {
        let ctx = self.context.borrow();

        let pos = self.mouse_transform.transform_point2(hit.uv);
        let x = ((pos.x * ctx.width as f32) as i32).max(0);
        let y = ((pos.y * ctx.height as f32) as i32).max(0);

        let ctx = self.context.borrow();
        ctx.wayvr
            .borrow_mut()
            .send_mouse_move(ctx.display, x as u32, y as u32);

        None
    }

    fn on_left(&mut self, _app: &mut state::AppState, _pointer: usize) {
        // Ignore event
    }

    fn on_pointer(&mut self, _app: &mut state::AppState, hit: &input::PointerHit, pressed: bool) {
        if let Some(index) = match hit.mode {
            input::PointerMode::Left => Some(wayvr::MouseIndex::Left),
            input::PointerMode::Middle => Some(wayvr::MouseIndex::Center),
            input::PointerMode::Right => Some(wayvr::MouseIndex::Right),
            _ => {
                // Unknown pointer event, ignore
                None
            }
        } {
            let ctx = self.context.borrow();
            let mut wayvr = ctx.wayvr.borrow_mut();
            if pressed {
                wayvr.send_mouse_down(ctx.display, index);
            } else {
                wayvr.send_mouse_up(ctx.display, index);
            }
        }
    }

    fn on_scroll(&mut self, _app: &mut state::AppState, _hit: &input::PointerHit, delta: f32) {
        let ctx = self.context.borrow();
        ctx.wayvr.borrow_mut().send_mouse_scroll(ctx.display, delta);
    }
}

pub struct WayVRRenderer {
    dmabuf_image: Option<Arc<vulkano::image::Image>>,
    view: Option<Arc<vulkano::image::view::ImageView>>,
    context: Rc<RefCell<WayVRContext>>,
    graphics: Arc<WlxGraphics>,
    width: u32,
    height: u32,
}

impl WayVRRenderer {
    pub fn new(
        app: &mut state::AppState,
        wvr: Rc<RefCell<wayvr::WayVR>>,
        width: u32,
        height: u32,
        processes: &[WayVRProcess],
    ) -> anyhow::Result<Self> {
        Ok(Self {
            context: Rc::new(RefCell::new(WayVRContext::new(
                wvr, width, height, processes,
            )?)),
            width,
            height,
            dmabuf_image: None,
            view: None,
            graphics: app.graphics.clone(),
        })
    }
}

impl WayVRRenderer {
    fn ensure_dmabuf(&mut self, data: wayvr::egl_data::DMAbufData) -> anyhow::Result<()> {
        if self.dmabuf_image.is_none() {
            // First init
            let mut planes = [FramePlane::default(); 4];
            planes[0].fd = Some(data.fd);
            planes[0].offset = data.offset as u32;
            planes[0].stride = data.stride;

            let frame = DmabufFrame {
                format: FrameFormat {
                    width: self.width,
                    height: self.height,
                    fourcc: FourCC {
                        value: data.mod_info.fourcc,
                    },
                    modifier: data.mod_info.modifiers[0], /* possibly not proper? */
                },
                num_planes: 1,
                planes,
            };

            let layouts: Vec<SubresourceLayout> = vec![SubresourceLayout {
                offset: data.offset as _,
                size: 0,
                row_pitch: data.stride as _,
                array_pitch: None,
                depth_pitch: None,
            }];

            let tex = self.graphics.dmabuf_texture_ex(
                frame,
                vulkano::image::ImageTiling::DrmFormatModifier,
                layouts,
                data.mod_info.modifiers,
            )?;
            self.dmabuf_image = Some(tex.clone());
            self.view = Some(vulkano::image::view::ImageView::new_default(tex).unwrap());
        }

        Ok(())
    }
}

impl OverlayRenderer for WayVRRenderer {
    fn init(&mut self, _app: &mut state::AppState) -> anyhow::Result<()> {
        Ok(())
    }

    fn pause(&mut self, _app: &mut state::AppState) -> anyhow::Result<()> {
        Ok(())
    }

    fn resume(&mut self, _app: &mut state::AppState) -> anyhow::Result<()> {
        Ok(())
    }

    fn render(&mut self, _app: &mut state::AppState) -> anyhow::Result<()> {
        let ctx = self.context.borrow();
        let mut wayvr = ctx.wayvr.borrow_mut();

        wayvr.tick_display(ctx.display)?;

        let dmabuf_data = wayvr
            .get_dmabuf_data(ctx.display)
            .ok_or(anyhow::anyhow!("Failed to fetch dmabuf data"))?
            .clone();

        drop(wayvr);
        drop(ctx);
        self.ensure_dmabuf(dmabuf_data.clone())?;

        Ok(())
    }

    fn view(&mut self) -> Option<Arc<vulkano::image::view::ImageView>> {
        self.view.clone()
    }

    fn extent(&mut self) -> Option<[u32; 3]> {
        self.view.as_ref().map(|view| view.image().extent())
    }
}

#[allow(dead_code)]
pub fn create_wayvr<O>(
    app: &mut state::AppState,
    width: u32,
    height: u32,
    processes: &[WayVRProcess],
) -> anyhow::Result<OverlayData<O>>
where
    O: Default,
{
    let transform = ui_transform(&[width, height]);

    let state = OverlayState {
        name: format!("WayVR Screen ({}x{})", width, height).into(),
        keyboard_focus: Some(KeyboardFocus::WayVR),
        want_visible: true,
        interactable: true,
        recenter: true,
        grabbable: true,
        spawn_scale: 1.0,
        spawn_point: vec3a(0.0, -0.5, 0.0),
        interaction_transform: transform,
        ..Default::default()
    };

    let wayvr = app.get_wayvr()?;

    let renderer = WayVRRenderer::new(app, wayvr, width, height, processes)?;
    let context = renderer.context.clone();

    let backend = Box::new(SplitOverlayBackend {
        renderer: Box::new(renderer),
        interaction: Box::new(WayVRInteractionHandler::new(context, Affine2::IDENTITY)),
    });

    Ok(OverlayData {
        state,
        backend,
        ..Default::default()
    })
}
