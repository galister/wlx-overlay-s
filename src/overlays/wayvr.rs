use glam::{vec3a, Affine2, Vec3, Vec3A};
use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};
use vulkano::image::SubresourceLayout;
use wlx_capture::frame::{DmabufFrame, FourCC, FrameFormat, FramePlane};

use crate::{
    backend::{
        common::{OverlayContainer, OverlaySelector},
        input::{self, InteractionHandler},
        overlay::{
            ui_transform, FrameTransform, OverlayData, OverlayID, OverlayRenderer, OverlayState,
            SplitOverlayBackend, Z_ORDER_DASHBOARD,
        },
        task::TaskType,
        wayvr::{
            self, display,
            server_ipc::{gen_args_vec, gen_env_vec},
            WayVR,
        },
    },
    config_wayvr,
    graphics::WlxGraphics,
    gui::modular::button::{WayVRAction, WayVRDisplayClickAction},
    state::{self, AppState, KeyboardFocus},
};

use super::toast::error_toast;

// Hard-coded for now
const DASHBOARD_WIDTH: u16 = 960;
const DASHBOARD_HEIGHT: u16 = 540;
const DASHBOARD_DISPLAY_NAME: &str = "_DASHBOARD";

pub struct WayVRContext {
    wayvr: Rc<RefCell<WayVRData>>,
    display: wayvr::display::DisplayHandle,
}

impl WayVRContext {
    pub fn new(
        wvr: Rc<RefCell<WayVRData>>,
        display: wayvr::display::DisplayHandle,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            wayvr: wvr.clone(),
            display,
        })
    }
}

struct OverlayToCreate {
    pub conf_display: config_wayvr::WayVRDisplay,
    pub disp_handle: display::DisplayHandle,
}

pub struct WayVRData {
    display_handle_map: HashMap<display::DisplayHandle, OverlayID>,
    overlays_to_create: Vec<OverlayToCreate>,
    dashboard_executed: bool,
    pub data: WayVR,
}

impl WayVRData {
    pub fn new(config: wayvr::Config) -> anyhow::Result<Self> {
        Ok(Self {
            display_handle_map: Default::default(),
            data: WayVR::new(config)?,
            overlays_to_create: Vec::new(),
            dashboard_executed: false,
        })
    }

    fn get_unique_display_name(&self, mut candidate: String) -> String {
        let mut num = 0;

        while !self
            .data
            .state
            .displays
            .vec
            .iter()
            .flatten()
            .any(|d| d.obj.name == candidate)
        {
            if num > 0 {
                candidate = format!("{} ({})", candidate, num);
            }
            num += 1;
        }

        candidate
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

        let wayvr = &mut ctx.wayvr.borrow_mut().data;

        if let Some(disp) = wayvr.state.displays.get(&ctx.display) {
            let pos = self.mouse_transform.transform_point2(hit.uv);
            let x = ((pos.x * disp.width as f32) as i32).max(0);
            let y = ((pos.y * disp.height as f32) as i32).max(0);

            let ctx = self.context.borrow();
            wayvr.state.send_mouse_move(ctx.display, x as u32, y as u32);
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
            let wayvr = &mut ctx.wayvr.borrow_mut().data;
            if pressed {
                wayvr.state.send_mouse_down(ctx.display, index);
            } else {
                wayvr.state.send_mouse_up(ctx.display, index);
            }
        }
    }

    fn on_scroll(&mut self, _app: &mut state::AppState, _hit: &input::PointerHit, delta: f32) {
        let ctx = self.context.borrow();
        ctx.wayvr
            .borrow_mut()
            .data
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
        app: &state::AppState,
        wvr: Rc<RefCell<WayVRData>>,
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

fn get_or_create_display_by_name(
    app: &mut AppState,
    wayvr: &mut WayVRData,
    disp_name: &str,
) -> anyhow::Result<display::DisplayHandle> {
    let disp_handle =
        if let Some(disp) = WayVR::get_display_by_name(&wayvr.data.state.displays, disp_name) {
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

            let disp_handle = wayvr.data.state.create_display(
                conf_display.width,
                conf_display.height,
                disp_name,
                conf_display.primary.unwrap_or(false),
            )?;

            wayvr.overlays_to_create.push(OverlayToCreate {
                conf_display,
                disp_handle,
            });

            disp_handle
        };

    Ok(disp_handle)
}

fn executable_exists_in_path(command: &str) -> bool {
    let Ok(path) = std::env::var("PATH") else {
        return false; // very unlikely to happen
    };
    for dir in path.split(':') {
        let exec_path = std::path::PathBuf::from(dir).join(command);
        if exec_path.exists() && exec_path.is_file() {
            return true; // executable found
        }
    }
    false
}

fn toggle_dashboard<O>(
    app: &mut AppState,
    overlays: &mut OverlayContainer<O>,
    wayvr: &mut WayVRData,
) -> anyhow::Result<()>
where
    O: Default,
{
    let conf_dash = &app.session.wayvr_config.dashboard;

    if !wayvr.dashboard_executed && !executable_exists_in_path(&conf_dash.exec) {
        anyhow::bail!("Executable \"{}\" not found", &conf_dash.exec);
    }

    let (newly_created, disp_handle) = wayvr.data.state.get_or_create_dashboard_display(
        DASHBOARD_WIDTH,
        DASHBOARD_HEIGHT,
        DASHBOARD_DISPLAY_NAME,
    )?;

    if newly_created {
        log::info!("Creating dashboard overlay");

        let mut overlay = create_overlay::<O>(
            app,
            wayvr,
            DASHBOARD_DISPLAY_NAME,
            OverlayToCreate {
                disp_handle,
                conf_display: config_wayvr::WayVRDisplay {
                    attach_to: None,
                    width: DASHBOARD_WIDTH,
                    height: DASHBOARD_HEIGHT,
                    scale: None,
                    rotation: None,
                    pos: None,
                    primary: None,
                },
            },
        )?;

        overlay.state.want_visible = true;
        overlay.state.spawn_scale = 1.25;
        overlay.state.z_order = Z_ORDER_DASHBOARD;
        overlay.state.reset(app, true);

        let conf_dash = &app.session.wayvr_config.dashboard;

        // FIXME: overlay curvature needs to be dispatched for some unknown reason, this value is not set otherwise
        app.tasks.enqueue(TaskType::Overlay(
            OverlaySelector::Id(overlay.state.id),
            Box::new(move |_app, o| {
                o.curvature = Some(0.15);
            }),
        ));

        overlays.add(overlay);

        let args_vec = match &conf_dash.args {
            Some(args) => gen_args_vec(args),
            None => vec![],
        };

        let env_vec = match &conf_dash.env {
            Some(env) => gen_env_vec(env),
            None => vec![],
        };

        // Start dashboard specified in the WayVR config
        let _process_handle_unused =
            wayvr
                .data
                .state
                .spawn_process(disp_handle, &conf_dash.exec, &args_vec, &env_vec)?;

        wayvr.dashboard_executed = true;

        return Ok(());
    }

    let display = wayvr.data.state.displays.get(&disp_handle).unwrap(); // safe
    let Some(overlay_id) = display.overlay_id else {
        anyhow::bail!("Overlay ID not set for dashboard display");
    };

    app.tasks.enqueue(TaskType::Overlay(
        OverlaySelector::Id(overlay_id),
        Box::new(move |app, o| {
            // Toggle visibility
            o.want_visible = !o.want_visible;
            if o.want_visible {
                o.reset(app, true);
            }
        }),
    ));

    Ok(())
}

fn create_overlay<O>(
    app: &mut AppState,
    data: &mut WayVRData,
    name: &str,
    cell: OverlayToCreate,
) -> anyhow::Result<OverlayData<O>>
where
    O: Default,
{
    let conf_display = &cell.conf_display;
    let disp_handle = cell.disp_handle;

    let mut overlay = create_wayvr_display_overlay::<O>(
        app,
        conf_display.width,
        conf_display.height,
        disp_handle,
        conf_display.scale.unwrap_or(1.0),
        name,
    )?;

    data.display_handle_map
        .insert(disp_handle, overlay.state.id);

    if let Some(attach_to) = &conf_display.attach_to {
        overlay.state.relative_to = attach_to.get_relative_to();
    }

    if let Some(rot) = &conf_display.rotation {
        overlay.state.spawn_rotation =
            glam::Quat::from_axis_angle(Vec3::from_slice(&rot.axis), f32::to_radians(rot.angle));
    }

    if let Some(pos) = &conf_display.pos {
        overlay.state.spawn_point = Vec3A::from_slice(pos);
    }

    let display = data.data.state.displays.get_mut(&disp_handle).unwrap(); // Never fails
    display.overlay_id = Some(overlay.state.id);

    Ok(overlay)
}

fn create_queued_displays<O>(
    app: &mut AppState,
    data: &mut WayVRData,
    overlays: &mut OverlayContainer<O>,
) -> anyhow::Result<()>
where
    O: Default,
{
    let overlays_to_create = std::mem::take(&mut data.overlays_to_create);

    for cell in overlays_to_create {
        let Some(disp) = data.data.state.displays.get(&cell.disp_handle) else {
            continue; // this shouldn't happen
        };

        let name = disp.name.clone();

        let overlay = create_overlay::<O>(app, data, name.as_str(), cell)?;
        overlays.add(overlay); // Insert freshly created WayVR overlay into wlx stack
    }

    Ok(())
}

pub fn tick_events<O>(app: &mut AppState, overlays: &mut OverlayContainer<O>) -> anyhow::Result<()>
where
    O: Default,
{
    let Some(r_wayvr) = app.wayvr.clone() else {
        return Ok(());
    };

    let mut wayvr = r_wayvr.borrow_mut();

    while let Some(signal) = wayvr.data.signals.read() {
        match signal {
            wayvr::WayVRSignal::DisplayHideRequest(display_handle) => {
                if let Some(overlay_id) = wayvr.display_handle_map.get(&display_handle) {
                    let overlay_id = *overlay_id;
                    wayvr.data.state.set_display_visible(display_handle, false);
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

    let res = wayvr.data.tick_events()?;
    drop(wayvr);

    for result in res {
        match result {
            wayvr::TickTask::NewExternalProcess(req) => {
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

                    let disp_handle = get_or_create_display_by_name(app, &mut wayvr, &disp_name)?;

                    wayvr.data.state.add_external_process(disp_handle, req.pid);

                    wayvr
                        .data
                        .state
                        .manager
                        .add_client(wayvr::client::WayVRClient {
                            client: req.client,
                            display_handle: disp_handle,
                            pid: req.pid,
                        });
                }
            }
            wayvr::TickTask::NewDisplay(cpar, disp_handle) => {
                log::info!("Creating new display with name \"{}\"", cpar.name);

                let mut wayvr = r_wayvr.borrow_mut();

                let unique_name = wayvr.get_unique_display_name(cpar.name);

                let disp_handle = match disp_handle {
                    Some(d) => d,
                    None => wayvr.data.state.create_display(
                        cpar.width,
                        cpar.height,
                        &unique_name,
                        false,
                    )?,
                };

                wayvr.overlays_to_create.push(OverlayToCreate {
                    disp_handle,
                    conf_display: config_wayvr::WayVRDisplay {
                        attach_to: Some(config_wayvr::AttachTo::from_packet(&cpar.attach_to)),
                        width: cpar.width,
                        height: cpar.height,
                        pos: None,
                        primary: None,
                        rotation: None,
                        scale: cpar.scale,
                    },
                });
            }
            wayvr::TickTask::DropOverlay(overlay_id) => {
                app.tasks
                    .enqueue(TaskType::DropOverlay(OverlaySelector::Id(overlay_id)));
            }
        }
    }

    let mut wayvr = r_wayvr.borrow_mut();
    create_queued_displays(app, &mut wayvr, overlays)?;

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
            if let Some(disp) = wayvr.data.state.displays.get(&ctx.display) {
                let frame = DmabufFrame {
                    format: FrameFormat {
                        width: disp.width as u32,
                        height: disp.height as u32,
                        fourcc: FourCC {
                            value: data.mod_info.fourcc,
                        },
                        modifier: data.mod_info.modifiers[0], /* possibly not proper? */
                        ..Default::default()
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
        let wayvr = &mut ctx.wayvr.borrow_mut().data;
        wayvr.state.set_display_visible(ctx.display, false);
        Ok(())
    }

    fn resume(&mut self, _app: &mut state::AppState) -> anyhow::Result<()> {
        let ctx = self.context.borrow_mut();
        let wayvr = &mut ctx.wayvr.borrow_mut().data;
        wayvr.state.set_display_visible(ctx.display, true);
        Ok(())
    }

    fn render(&mut self, _app: &mut state::AppState) -> anyhow::Result<()> {
        let ctx = self.context.borrow();
        let mut wayvr = ctx.wayvr.borrow_mut();

        match wayvr.data.tick_display(ctx.display) {
            Ok(_) => {}
            Err(e) => {
                log::error!("tick_display failed: {}", e);
                return Ok(()); // do not proceed further
            }
        }

        let dmabuf_data = wayvr
            .data
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

    fn frame_transform(&mut self) -> Option<FrameTransform> {
        self.view.as_ref().map(|view| FrameTransform {
            extent: view.image().extent(),
            ..Default::default()
        })
    }
}

#[allow(dead_code)]
pub fn create_wayvr_display_overlay<O>(
    app: &mut state::AppState,
    display_width: u16,
    display_height: u16,
    display_handle: wayvr::display::DisplayHandle,
    display_scale: f32,
    name: &str,
) -> anyhow::Result<OverlayData<O>>
where
    O: Default,
{
    let transform = ui_transform(&[display_width as u32, display_height as u32]);

    let state = OverlayState {
        name: format!("WayVR - {}", name).into(),
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

fn show_display<O>(wayvr: &mut WayVRData, overlays: &mut OverlayContainer<O>, display_name: &str)
where
    O: Default,
{
    if let Some(display) = WayVR::get_display_by_name(&wayvr.data.state.displays, display_name) {
        if let Some(overlay_id) = wayvr.display_handle_map.get(&display) {
            if let Some(overlay) = overlays.mut_by_id(*overlay_id) {
                overlay.state.want_visible = true;
            }
        }

        wayvr.data.state.set_display_visible(display, true);
    }
}

fn action_app_click<O>(
    app: &mut AppState,
    overlays: &mut OverlayContainer<O>,
    catalog_name: &Arc<str>,
    app_name: &Arc<str>,
) -> anyhow::Result<()>
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

        let disp_handle =
            get_or_create_display_by_name(app, &mut wayvr, &app_entry.target_display)?;

        let args_vec = match &app_entry.args {
            Some(args) => gen_args_vec(args),
            None => vec![],
        };

        let env_vec = match &app_entry.env {
            Some(env) => gen_env_vec(env),
            None => vec![],
        };

        // Terminate existing process if required
        if let Some(process_handle) =
            wayvr
                .data
                .state
                .process_query(disp_handle, &app_entry.exec, &args_vec, &env_vec)
        {
            // Terminate process
            wayvr.data.terminate_process(process_handle);
        } else {
            // Spawn process
            wayvr
                .data
                .state
                .spawn_process(disp_handle, &app_entry.exec, &args_vec, &env_vec)?;

            show_display::<O>(&mut wayvr, overlays, app_entry.target_display.as_str());
        }
    }

    Ok(())
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

    let Some(handle) = WayVR::get_display_by_name(&wayvr.data.state.displays, display_name) else {
        return Ok(());
    };

    let Some(display) = wayvr.data.state.displays.get_mut(&handle) else {
        return Ok(());
    };

    let Some(overlay_id) = display.overlay_id else {
        return Ok(());
    };

    let Some(overlay) = overlays.mut_by_id(overlay_id) else {
        return Ok(());
    };

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
            if let Err(e) = action_app_click(app, overlays, catalog_name, app_name) {
                // Happens if something went wrong with initialization
                // or input exec path is invalid. Do nothing, just print an error
                error_toast(app, "action_app_click failed", e);
            }
        }
        WayVRAction::DisplayClick {
            display_name,
            action,
        } => {
            if let Err(e) = action_display_click::<O>(app, overlays, display_name, action) {
                error_toast(app, "action_display_click failed", e);
            }
        }
        WayVRAction::ToggleDashboard => {
            let wayvr = app.get_wayvr().unwrap(); /* safe */
            let mut wayvr = wayvr.borrow_mut();

            if let Err(e) = toggle_dashboard::<O>(app, overlays, &mut wayvr) {
                error_toast(app, "toggle_dashboard failed", e);
            }
        }
    }
}
