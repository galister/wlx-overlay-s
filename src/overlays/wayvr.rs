use glam::{vec3a, Affine2, Vec3, Vec3A};
use serde::Deserialize;
use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};
use vulkano::image::SubresourceLayout;
use wlx_capture::frame::{DmabufFrame, FourCC, FrameFormat, FramePlane};

use crate::{
    backend::{
        common::{OverlayContainer, OverlaySelector},
        input::{self, InteractionHandler},
        overlay::{
            ui_transform, OverlayData, OverlayID, OverlayRenderer, OverlayState,
            SplitOverlayBackend,
        },
        task::TaskType,
        wayvr::{self, display, WayVR},
    },
    graphics::WlxGraphics,
    state::{self, AppState, KeyboardFocus},
};

pub struct WayVRContext {
    wayvr: Rc<RefCell<WayVRState>>,
    display: wayvr::display::DisplayHandle,
}

impl WayVRContext {
    pub fn new(
        wvr: Rc<RefCell<WayVRState>>,
        display: wayvr::display::DisplayHandle,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            wayvr: wvr.clone(),
            display,
        })
    }
}

pub struct WayVRState {
    pub display_handle_map: HashMap<display::DisplayHandle, OverlayID>,
    pub state: WayVR,
}

impl WayVRState {
    pub fn new(config: wayvr::Config) -> anyhow::Result<Self> {
        Ok(Self {
            display_handle_map: Default::default(),
            state: WayVR::new(config)?,
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

        let wayvr = &mut ctx.wayvr.borrow_mut().state;
        if let Some(disp) = wayvr.displays.get(&ctx.display) {
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
            let wayvr = &mut ctx.wayvr.borrow_mut().state;
            if pressed {
                wayvr.send_mouse_down(ctx.display, index);
            } else {
                wayvr.send_mouse_up(ctx.display, index);
            }
        }
    }

    fn on_scroll(&mut self, _app: &mut state::AppState, _hit: &input::PointerHit, delta: f32) {
        let ctx = self.context.borrow();
        ctx.wayvr
            .borrow_mut()
            .state
            .send_mouse_scroll(ctx.display, delta);
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
        wvr: Rc<RefCell<WayVRState>>,
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

fn get_or_create_display<O>(
    app: &mut AppState,
    wayvr: &mut WayVRState,
    disp_name: &str,
) -> anyhow::Result<(display::DisplayHandle, Option<OverlayData<O>>)>
where
    O: Default,
{
    let created_overlay: Option<OverlayData<O>>;

    let disp_handle =
        if let Some(disp) = WayVR::get_display_by_name(&wayvr.state.displays, disp_name) {
            created_overlay = None;
            disp
        } else {
            let conf_display = app
                .session
                .wayvr_config
                .get_display(disp_name)
                .ok_or(anyhow::anyhow!(
                    "Cannot find display named \"{}\"",
                    disp_name
                ))?
                .clone();

            let disp_handle = wayvr.state.create_display(
                conf_display.width,
                conf_display.height,
                disp_name,
                conf_display.primary.unwrap_or(false),
            )?;

            let mut overlay = create_wayvr_display_overlay::<O>(
                app,
                conf_display.width,
                conf_display.height,
                disp_handle,
                conf_display.scale.unwrap_or(1.0),
            )?;

            wayvr
                .display_handle_map
                .insert(disp_handle, overlay.state.id);

            if let Some(attach_to) = &conf_display.attach_to {
                overlay.state.relative_to = attach_to.get_relative_to();
            }

            if let Some(rot) = &conf_display.rotation {
                overlay.state.spawn_rotation = glam::Quat::from_axis_angle(
                    Vec3::from_slice(&rot.axis),
                    f32::to_radians(rot.angle),
                );
            }

            if let Some(pos) = &conf_display.pos {
                overlay.state.spawn_point = Vec3A::from_slice(pos);
            }

            let display = wayvr.state.displays.get_mut(&disp_handle).unwrap(); // Never fails
            display.overlay_id = Some(overlay.state.id);

            created_overlay = Some(overlay);

            disp_handle
        };

    Ok((disp_handle, created_overlay))
}

pub fn tick_events<O>(app: &mut AppState, overlays: &mut OverlayContainer<O>) -> anyhow::Result<()>
where
    O: Default,
{
    if let Some(r_wayvr) = app.wayvr.clone() {
        let mut wayvr = r_wayvr.borrow_mut();
        while let Some(signal) = wayvr.state.signals.read() {
            match signal {
                wayvr::WayVRSignal::DisplayHideRequest(display_handle) => {
                    if let Some(overlay_id) = wayvr.display_handle_map.get(&display_handle) {
                        let overlay_id = *overlay_id;
                        wayvr.state.set_display_visible(display_handle, false);
                        app.tasks.enqueue(TaskType::Overlay(
                            OverlaySelector::Id(overlay_id),
                            Box::new(move |_app, o| {
                                o.want_visible = false;
                            }),
                        ));
                    }
                }
            }
        }

        let res = wayvr.state.tick_events()?;
        drop(wayvr);

        for result in res {
            match result {
                wayvr::TickResult::NewExternalProcess(req) => {
                    let config = &app.session.wayvr_config;

                    let disp_name = if let Some(display_name) = req.env.display_name {
                        config
                            .get_display(display_name.as_str())
                            .map(|_| display_name)
                    } else {
                        config
                            .get_default_display()
                            .map(|(display_name, _)| display_name)
                    };

                    if let Some(disp_name) = disp_name {
                        let mut wayvr = r_wayvr.borrow_mut();

                        log::info!("Registering external process with PID {}", req.pid);

                        let (disp_handle, created_overlay) =
                            get_or_create_display::<O>(app, &mut wayvr, &disp_name)?;

                        wayvr.state.add_external_process(disp_handle, req.pid);

                        wayvr.state.manager.add_client(wayvr::client::WayVRClient {
                            client: req.client,
                            display_handle: disp_handle,
                            pid: req.pid,
                        });

                        if let Some(created_overlay) = created_overlay {
                            overlays.add(created_overlay);
                        }
                    }
                }
            }
        }
    }

    Ok(())
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
            if let Some(disp) = wayvr.state.displays.get(&ctx.display) {
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
        let ctx = self.context.borrow_mut();
        let wayvr = &mut ctx.wayvr.borrow_mut().state;
        wayvr.set_display_visible(ctx.display, false);
        Ok(())
    }

    fn resume(&mut self, _app: &mut state::AppState) -> anyhow::Result<()> {
        let ctx = self.context.borrow_mut();
        let wayvr = &mut ctx.wayvr.borrow_mut().state;
        wayvr.set_display_visible(ctx.display, true);
        Ok(())
    }

    fn render(&mut self, _app: &mut state::AppState) -> anyhow::Result<()> {
        let ctx = self.context.borrow();
        let mut wayvr = ctx.wayvr.borrow_mut();

        wayvr.state.tick_display(ctx.display)?;

        let dmabuf_data = wayvr
            .state
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
pub fn create_wayvr_display_overlay<O>(
    app: &mut state::AppState,
    display_width: u32,
    display_height: u32,
    display_handle: wayvr::display::DisplayHandle,
    display_scale: f32,
) -> anyhow::Result<OverlayData<O>>
where
    O: Default,
{
    let transform = ui_transform(&[display_width, display_height]);

    let state = OverlayState {
        name: format!("WayVR Screen ({}x{})", display_width, display_height).into(),
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

#[derive(Deserialize, Clone)]
pub enum WayVRDisplayClickAction {
    ToggleVisibility,
    Reset,
}

#[derive(Deserialize, Clone)]
pub enum WayVRAction {
    AppClick {
        catalog_name: Arc<str>,
        app_name: Arc<str>,
    },
    DisplayClick {
        display_name: Arc<str>,
        action: WayVRDisplayClickAction,
    },
}

fn show_display<O>(wayvr: &mut WayVRState, overlays: &mut OverlayContainer<O>, display_name: &str)
where
    O: Default,
{
    if let Some(display) = WayVR::get_display_by_name(&wayvr.state.displays, display_name) {
        if let Some(overlay_id) = wayvr.display_handle_map.get(&display) {
            if let Some(overlay) = overlays.mut_by_id(*overlay_id) {
                overlay.state.want_visible = true;
            }
        }

        wayvr.state.set_display_visible(display, true);
    }
}

fn action_app_click<O>(
    app: &mut AppState,
    overlays: &mut OverlayContainer<O>,
    catalog_name: &Arc<str>,
    app_name: &Arc<str>,
) -> anyhow::Result<Option<OverlayData<O>>>
where
    O: Default,
{
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

        let (disp_handle, created_overlay) =
            get_or_create_display::<O>(app, &mut wayvr, &app_entry.target_display)?;

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

        // Terminate existing process if required
        if let Some(process_handle) =
            wayvr
                .state
                .process_query(disp_handle, &app_entry.exec, &args_vec, &env_vec)
        {
            // Terminate process
            wayvr.state.terminate_process(process_handle);
        } else {
            // Spawn process
            wayvr
                .state
                .spawn_process(disp_handle, &app_entry.exec, &args_vec, &env_vec)?;

            show_display(&mut wayvr, overlays, &app_entry.target_display.as_str());
        }
        Ok(created_overlay)
    } else {
        Ok(None)
    }
}

pub fn action_display_click<O>(
    app: &mut AppState,
    overlays: &mut OverlayContainer<O>,
    display_name: &Arc<str>,
    action: &WayVRDisplayClickAction,
) -> anyhow::Result<()>
where
    O: Default,
{
    let wayvr = app.get_wayvr()?;
    let mut wayvr = wayvr.borrow_mut();

    if let Some(handle) = WayVR::get_display_by_name(&wayvr.state.displays, display_name) {
        if let Some(display) = wayvr.state.displays.get_mut(&handle) {
            if let Some(overlay_id) = display.overlay_id {
                if let Some(overlay) = overlays.mut_by_id(overlay_id) {
                    match action {
                        WayVRDisplayClickAction::ToggleVisibility => {
                            // Toggle visibility
                            overlay.state.want_visible = !overlay.state.want_visible;
                        }
                        WayVRDisplayClickAction::Reset => {
                            // Show it at the front
                            overlay.state.want_visible = true;
                            overlay.state.reset(app, true);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

pub fn wayvr_action<O>(app: &mut AppState, overlays: &mut OverlayContainer<O>, action: &WayVRAction)
where
    O: Default,
{
    match action {
        WayVRAction::AppClick {
            catalog_name,
            app_name,
        } => {
            match action_app_click(app, overlays, catalog_name, app_name) {
                Ok(res) => {
                    if let Some(created_overlay) = res {
                        overlays.add(created_overlay);
                    }
                }
                Err(e) => {
                    // Happens if something went wrong with initialization
                    // or input exec path is invalid. Do nothing, just print an error
                    log::error!("action_app_click failed: {}", e);
                }
            }
        }
        WayVRAction::DisplayClick {
            display_name,
            action,
        } => {
            if let Err(e) = action_display_click::<O>(app, overlays, display_name, action) {
                log::error!("action_display_click failed: {}", e);
            }
        }
    }
}
