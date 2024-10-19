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
    state::{self, AppState, KeyboardFocus},
};

pub struct WayVRContext {
    wayvr: Rc<RefCell<wayvr::WayVR>>,
    display: wayvr::display::DisplayHandle,
}

impl WayVRContext {
    pub fn new(
        wvr: Rc<RefCell<wayvr::WayVR>>,
        display: wayvr::display::DisplayHandle,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            wayvr: wvr.clone(),
            display,
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

        let mut wayvr = ctx.wayvr.borrow_mut();
        if let Some(disp) = wayvr.get_display_by_handle(ctx.display) {
            let pos = self.mouse_transform.transform_point2(hit.uv);
            let x = ((pos.x * disp.width as f32) as i32).max(0);
            let y = ((pos.y * disp.height as f32) as i32).max(0);

            let ctx = self.context.borrow();
            wayvr.send_mouse_move(ctx.display, x as u32, y as u32);
        }

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
}

impl WayVRRenderer {
    pub fn new(
        app: &mut state::AppState,
        wvr: Rc<RefCell<wayvr::WayVR>>,
        display: wayvr::display::DisplayHandle,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            context: Rc::new(RefCell::new(WayVRContext::new(wvr, display)?)),
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

            let ctx = self.context.borrow_mut();
            let wayvr = ctx.wayvr.borrow_mut();
            if let Some(disp) = wayvr.get_display_by_handle(ctx.display) {
                let frame = DmabufFrame {
                    format: FrameFormat {
                        width: disp.width,
                        height: disp.height,
                        fourcc: FourCC {
                            value: data.mod_info.fourcc,
                        },
                        modifier: data.mod_info.modifiers[0], /* possibly not proper? */
                    },
                    num_planes: 1,
                    planes,
                };

                drop(wayvr);

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
            } else {
                anyhow::bail!("Failed to fetch WayVR display")
            }
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
    display: &wayvr::display::Display,
    display_handle: wayvr::display::DisplayHandle,
    display_scale: f32,
) -> anyhow::Result<OverlayData<O>>
where
    O: Default,
{
    let transform = ui_transform(&[display.width, display.height]);

    let state = OverlayState {
        name: format!("WayVR Screen ({}x{})", display.width, display.height).into(),
        keyboard_focus: Some(KeyboardFocus::WayVR),
        want_visible: true,
        interactable: true,
        grabbable: true,
        spawn_scale: display_scale,
        spawn_point: vec3a(0.0, -0.1, -1.0),
        interaction_transform: transform,
        ..Default::default()
    };

    let wayvr = app.get_wayvr()?;

    let renderer = WayVRRenderer::new(app, wayvr, display_handle)?;
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

fn action_wayvr_internal<O>(
    catalog_name: &Arc<str>,
    app_name: &Arc<str>,
    app: &mut AppState,
) -> anyhow::Result<Option<OverlayData<O>>>
where
    O: Default,
{
    use crate::overlays::wayvr::create_wayvr;

    let mut created_overlay: Option<OverlayData<O>> = None;

    let wayvr = app.get_wayvr()?.clone();

    let catalog = app
        .session
        .wayvr_config
        .get_catalog(catalog_name)
        .ok_or(anyhow::anyhow!(
            "Failed to get catalog \"{}\"",
            catalog_name
        ))?
        .clone();

    if let Some(app_entry) = catalog.get_app(app_name) {
        let mut wayvr = wayvr.borrow_mut();

        let disp_handle = if let Some(disp) = wayvr.get_display_by_name(&app_entry.target_display) {
            disp
        } else {
            let conf_display = app
                .session
                .wayvr_config
                .get_display(&app_entry.target_display)
                .ok_or(anyhow::anyhow!(
                    "Cannot find display named \"{}\"",
                    app_entry.target_display
                ))?;

            let display_handle = wayvr.create_display(
                conf_display.width,
                conf_display.height,
                &app_entry.target_display,
            )?;
            let display = wayvr.get_display_by_handle(display_handle).unwrap(); // Never fails
            created_overlay = Some(create_wayvr::<O>(
                app,
                display,
                display_handle,
                conf_display.scale,
            )?);
            display_handle
        };

        // Parse additional args
        let args_vec: Vec<&str> = if let Some(args) = &app_entry.args {
            args.as_str().split_whitespace().collect()
        } else {
            vec![]
        };

        // Parse additional env
        let env_vec: Vec<(&str, &str)> = if let Some(env) = &app_entry.env {
            // splits "FOO=BAR=TEST,123" into (&"foo", &"bar=test,123")
            env.iter()
                .filter_map(|e| e.as_str().split_once('='))
                .collect()
        } else {
            vec![]
        };

        wayvr.spawn_process(disp_handle, &app_entry.exec, &args_vec, &env_vec)?
    }

    Ok(created_overlay)
}

// Returns newly created overlay (if needed)
pub fn action_wayvr<O>(
    catalog_name: &Arc<str>,
    app_name: &Arc<str>,
    app: &mut AppState,
) -> Option<OverlayData<O>>
where
    O: Default,
{
    match action_wayvr_internal(catalog_name, app_name, app) {
        Ok(res) => res,
        Err(e) => {
            // Happens if something went wrong with initialization
            // or input exec path is invalid. Do nothing, just print an error
            log::error!("action_wayvr failed: {}", e);
            None
        }
    }
}
