use glam::{vec3a, Affine2, Vec3, Vec3A};
use smallvec::smallvec;
use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};
use vulkano::{
    buffer::{BufferUsage, Subbuffer},
    command_buffer::CommandBufferUsage,
    format::Format,
    image::{view::ImageView, Image, ImageTiling, SubresourceLayout},
};
use wayvr_ipc::packet_server::{self, PacketServer, WvrStateChanged};
use wgui::gfx::{
    pass::WGfxPass,
    pipeline::{WGfxPipeline, WPipelineCreateInfo},
    WGfx,
};
use wlx_capture::frame::{DmabufFrame, FourCC, FrameFormat, FramePlane};

use crate::{
    backend::{
        common::{OverlayContainer, OverlaySelector},
        input::{self},
        overlay::{
            ui_transform, FrameMeta, OverlayBackend, OverlayData, OverlayID, OverlayState,
            ShouldRender, Z_ORDER_DASHBOARD,
        },
        task::TaskType,
        wayvr::{
            self, display,
            server_ipc::{gen_args_vec, gen_env_vec},
            WayVR, WayVRAction, WayVRDisplayClickAction,
        },
    },
    config_wayvr,
    graphics::{dmabuf::WGfxDmabuf, CommandBuffers, Vert2Uv},
    state::{self, AppState},
    subsystem::input::KeyboardFocus,
};

use super::toast::error_toast;

// Hard-coded for now
const DASHBOARD_WIDTH: u16 = 1920;
const DASHBOARD_HEIGHT: u16 = 1080;
const DASHBOARD_DISPLAY_NAME: &str = "_DASHBOARD";

pub struct WayVRContext {
    wayvr: Rc<RefCell<WayVRData>>,
    display: wayvr::display::DisplayHandle,
}

impl WayVRContext {
    pub const fn new(wvr: Rc<RefCell<WayVRData>>, display: wayvr::display::DisplayHandle) -> Self {
        Self {
            wayvr: wvr,
            display,
        }
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
    pending_haptics: Option<input::Haptics>,
}

impl WayVRData {
    pub fn new(config: wayvr::Config) -> anyhow::Result<Self> {
        Ok(Self {
            display_handle_map: HashMap::default(),
            data: WayVR::new(config)?,
            overlays_to_create: Vec::new(),
            dashboard_executed: false,
            pending_haptics: None,
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
                candidate = format!("{candidate} ({num})");
            }
            num += 1;
        }

        candidate
    }
}

struct ImageData {
    vk_image: Arc<Image>,
    vk_image_view: Arc<ImageView>,
}

pub struct WayVRBackend {
    pipeline: Arc<WGfxPipeline<Vert2Uv>>,
    pass: WGfxPass<Vert2Uv>,
    buf_alpha: Subbuffer<[f32]>,
    image: Option<ImageData>,
    context: Rc<RefCell<WayVRContext>>,
    graphics: Arc<WGfx>,
    resolution: [u16; 2],
    mouse_transform: Affine2,
    interaction_transform: Option<Affine2>,
}

impl WayVRBackend {
    pub fn new(
        app: &state::AppState,
        wvr: Rc<RefCell<WayVRData>>,
        display: wayvr::display::DisplayHandle,
        resolution: [u16; 2],
    ) -> anyhow::Result<Self> {
        let pipeline = app.gfx.create_pipeline(
            app.gfx_extras.shaders.get("vert_quad").unwrap(), // want panic
            app.gfx_extras.shaders.get("frag_srgb").unwrap(), // want panic
            WPipelineCreateInfo::new(app.gfx.surface_format)
                .use_updatable_descriptors(smallvec![0]),
        )?;

        let buf_alpha = app
            .gfx
            .empty_buffer(BufferUsage::TRANSFER_DST | BufferUsage::UNIFORM_BUFFER, 1)?;

        let set0 = pipeline.uniform_sampler(
            0,
            app.gfx_extras.fallback_image.clone(),
            app.gfx.texture_filter,
        )?;
        let set1 = pipeline.buffer(1, buf_alpha.clone())?;
        let pass = pipeline.create_pass(
            [resolution[0] as _, resolution[1] as _],
            app.gfx_extras.quad_verts.clone(),
            0..4,
            0..1,
            vec![set0, set1],
            &Default::default(),
        )?;

        Ok(Self {
            pipeline,
            pass,
            buf_alpha,
            context: Rc::new(RefCell::new(WayVRContext::new(wvr, display))),
            graphics: app.gfx.clone(),
            image: None,
            resolution,
            mouse_transform: Affine2::IDENTITY,
            interaction_transform: Some(ui_transform([resolution[0] as _, resolution[1] as _])), //TODO:dynamic
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
                .ok_or_else(|| anyhow::anyhow!("Cannot find display named \"{}\"", disp_name))?
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

pub fn executable_exists_in_path(command: &str) -> bool {
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
    let Some(conf_dash) = app.session.wayvr_config.dashboard.clone() else {
        anyhow::bail!("Dashboard is not configured");
    };

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

        overlay.state.curvature = Some(0.15);
        overlay.state.want_visible = true;
        overlay.state.spawn_scale = 2.0;
        overlay.state.spawn_point = vec3a(0.0, -0.35, -1.75);
        overlay.state.z_order = Z_ORDER_DASHBOARD;
        overlay.state.reset(app, true);

        overlays.add(overlay);

        let args_vec = &conf_dash
            .args
            .as_ref()
            .map_or_else(Vec::new, |args| gen_args_vec(args.as_str()));

        let env_vec = &conf_dash
            .env
            .as_ref()
            .map_or_else(Vec::new, |env| gen_env_vec(env));

        let mut userdata = HashMap::new();
        userdata.insert(String::from("type"), String::from("dashboard"));

        // Start dashboard specified in the WayVR config
        let _process_handle_unused = wayvr.data.state.spawn_process(
            disp_handle,
            &conf_dash.exec,
            args_vec,
            env_vec,
            conf_dash.working_dir.as_deref(),
            userdata,
        )?;

        wayvr.dashboard_executed = true;

        return Ok(());
    }

    let display = wayvr.data.state.displays.get(&disp_handle).unwrap(); // safe
    let Some(overlay_id) = display.overlay_id else {
        anyhow::bail!("Overlay ID not set for dashboard display");
    };

    let cur_visibility = !display.visible;

    wayvr
        .data
        .ipc_server
        .broadcast(PacketServer::WvrStateChanged(if cur_visibility {
            WvrStateChanged::DashboardShown
        } else {
            WvrStateChanged::DashboardHidden
        }));

    app.tasks.enqueue(TaskType::Overlay(
        OverlaySelector::Id(overlay_id),
        Box::new(move |app, o| {
            // Toggle visibility
            o.want_visible = cur_visibility;
            if cur_visibility {
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
        overlay.state.positioning = attach_to.get_positioning();
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

#[allow(clippy::too_many_lines)]
pub fn tick_events<O>(app: &mut AppState, overlays: &mut OverlayContainer<O>) -> anyhow::Result<()>
where
    O: Default,
{
    let Some(r_wayvr) = app.wayvr.clone() else {
        return Ok(());
    };

    let mut wayvr = r_wayvr.borrow_mut();

    while let Some(signal) = wayvr.data.state.signals.read() {
        match signal {
            wayvr::WayVRSignal::DisplayVisibility(display_handle, visible) => {
                if let Some(overlay_id) = wayvr.display_handle_map.get(&display_handle) {
                    let overlay_id = *overlay_id;
                    wayvr
                        .data
                        .state
                        .set_display_visible(display_handle, visible);
                    app.tasks.enqueue(TaskType::Overlay(
                        OverlaySelector::Id(overlay_id),
                        Box::new(move |_app, o| {
                            o.want_visible = visible;
                        }),
                    ));
                }
            }
            wayvr::WayVRSignal::DisplayWindowLayout(display_handle, layout) => {
                wayvr.data.state.set_display_layout(display_handle, layout);
            }
            wayvr::WayVRSignal::BroadcastStateChanged(packet) => {
                wayvr
                    .data
                    .ipc_server
                    .broadcast(packet_server::PacketServer::WvrStateChanged(packet));
            }
            wayvr::WayVRSignal::DropOverlay(overlay_id) => {
                app.tasks
                    .enqueue(TaskType::DropOverlay(OverlaySelector::Id(overlay_id)));
            }
            wayvr::WayVRSignal::Haptics(haptics) => {
                wayvr.pending_haptics = Some(haptics);
            }
        }
    }

    let res = wayvr.data.tick_events(app)?;
    drop(wayvr);

    for result in res {
        match result {
            wayvr::TickTask::NewExternalProcess(request) => {
                let config = &app.session.wayvr_config;

                let disp_name = request.env.display_name.map_or_else(
                    || {
                        config
                            .get_default_display()
                            .map(|(display_name, _)| display_name)
                    },
                    |display_name| {
                        config
                            .get_display(display_name.as_str())
                            .map(|_| display_name)
                    },
                );

                if let Some(disp_name) = disp_name {
                    let mut wayvr = r_wayvr.borrow_mut();

                    log::info!("Registering external process with PID {}", request.pid);

                    let disp_handle = get_or_create_display_by_name(app, &mut wayvr, &disp_name)?;

                    wayvr
                        .data
                        .state
                        .add_external_process(disp_handle, request.pid);

                    wayvr
                        .data
                        .state
                        .manager
                        .add_client(wayvr::client::WayVRClient {
                            client: request.client,
                            display_handle: disp_handle,
                            pid: request.pid,
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
        }
    }

    let mut wayvr = r_wayvr.borrow_mut();
    create_queued_displays(app, &mut wayvr, overlays)?;

    Ok(())
}

impl WayVRBackend {
    fn ensure_software_data(
        &mut self,
        data: &wayvr::egl_data::RenderSoftwarePixelsData,
    ) -> anyhow::Result<()> {
        let mut upload = self
            .graphics
            .create_xfer_command_buffer(CommandBufferUsage::OneTimeSubmit)?;

        let tex = upload.upload_image(
            u32::from(data.width),
            u32::from(data.height),
            Format::R8G8B8A8_UNORM,
            &data.data,
        )?;

        // FIXME: can we use _buffers_ here?
        upload.build_and_execute_now()?;

        //buffers.push(upload.build()?);
        self.image = Some(ImageData {
            vk_image: tex.clone(),
            vk_image_view: ImageView::new_default(tex).unwrap(),
        });
        Ok(())
    }

    fn ensure_dmabuf_data(
        &mut self,
        data: &wayvr::egl_data::RenderDMAbufData,
    ) -> anyhow::Result<()> {
        if self.image.is_some() {
            return Ok(()); // already initialized and automatically updated due to direct zero-copy textue access
        }

        // First init
        let mut planes = [FramePlane::default(); 4];
        planes[0].fd = Some(data.fd);
        planes[0].offset = data.offset as u32;
        planes[0].stride = data.stride;

        let ctx = self.context.borrow_mut();
        let wayvr = ctx.wayvr.borrow_mut();
        let Some(disp) = wayvr.data.state.displays.get(&ctx.display) else {
            anyhow::bail!("Failed to fetch WayVR display")
        };

        let frame = DmabufFrame {
            format: FrameFormat {
                width: u32::from(disp.width),
                height: u32::from(disp.height),
                fourcc: FourCC {
                    value: data.mod_info.fourcc,
                },
                modifier: data.mod_info.modifiers[0], /* possibly not proper? */
                ..Default::default()
            },
            num_planes: 1,
            planes,
            ..Default::default()
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
            ImageTiling::DrmFormatModifier,
            layouts,
            &data.mod_info.modifiers,
        )?;

        self.image = Some(ImageData {
            vk_image: tex.clone(),
            vk_image_view: ImageView::new_default(tex).unwrap(),
        });
        Ok(())
    }
}

impl OverlayBackend for WayVRBackend {
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

    fn should_render(&mut self, _app: &mut AppState) -> anyhow::Result<ShouldRender> {
        let ctx = self.context.borrow();
        let mut wayvr = ctx.wayvr.borrow_mut();
        let redrawn = match wayvr.data.render_display(ctx.display) {
            Ok(r) => r,
            Err(e) => {
                log::error!("render_display failed: {e}");
                return Ok(ShouldRender::Unable);
            }
        };

        if redrawn {
            Ok(ShouldRender::Should)
        } else {
            Ok(ShouldRender::Can)
        }
    }

    fn render(
        &mut self,
        app: &mut state::AppState,
        tgt: Arc<ImageView>,
        buf: &mut CommandBuffers,
        alpha: f32,
    ) -> anyhow::Result<bool> {
        let ctx = self.context.borrow();
        let wayvr = ctx.wayvr.borrow_mut();

        let data = wayvr
            .data
            .state
            .get_render_data(ctx.display)
            .ok_or_else(|| anyhow::anyhow!("Failed to fetch render data"))?
            .clone();

        drop(wayvr);
        drop(ctx);

        match data {
            wayvr::egl_data::RenderData::Dmabuf(data) => {
                self.ensure_dmabuf_data(&data)?;
            }
            wayvr::egl_data::RenderData::Software(data) => {
                if let Some(new_frame) = &data {
                    self.ensure_software_data(new_frame)?;
                }
            }
        }

        let Some(image) = self.image.as_ref() else {
            return Ok(false);
        };

        self.pass
            .update_sampler(0, image.vk_image_view.clone(), self.graphics.texture_filter)?;
        self.buf_alpha.write()?[0] = alpha;

        let mut cmd_buffer = app
            .gfx
            .create_gfx_command_buffer(CommandBufferUsage::OneTimeSubmit)?;
        cmd_buffer.begin_rendering(
            tgt,
            wgui::gfx::cmd::WGfxClearMode::Clear([0.0, 0.0, 0.0, 1.0]),
        )?;
        cmd_buffer.run_ref(&self.pass)?;
        cmd_buffer.end_rendering()?;
        buf.push(cmd_buffer.build()?);

        Ok(true)
    }

    fn frame_meta(&mut self) -> Option<FrameMeta> {
        Some(FrameMeta {
            extent: [self.resolution[0] as u32, self.resolution[1] as u32, 1],
            ..Default::default()
        })
    }

    fn on_hover(
        &mut self,
        _app: &mut state::AppState,
        hit: &input::PointerHit,
    ) -> Option<input::Haptics> {
        let ctx = self.context.borrow();

        let wayvr = &mut ctx.wayvr.borrow_mut();

        if let Some(disp) = wayvr.data.state.displays.get(&ctx.display) {
            let pos = self.mouse_transform.transform_point2(hit.uv);
            let x = ((pos.x * f32::from(disp.width)) as i32).max(0);
            let y = ((pos.y * f32::from(disp.height)) as i32).max(0);

            let ctx = self.context.borrow();
            wayvr
                .data
                .state
                .send_mouse_move(ctx.display, x as u32, y as u32);
        }

        wayvr.pending_haptics.take()
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
                wayvr.state.send_mouse_up(index);
            }
        }
    }

    fn on_scroll(
        &mut self,
        _app: &mut state::AppState,
        _hit: &input::PointerHit,
        delta_y: f32,
        delta_x: f32,
    ) {
        let ctx = self.context.borrow();
        ctx.wayvr
            .borrow_mut()
            .data
            .state
            .send_mouse_scroll(delta_y, delta_x);
    }

    fn get_interaction_transform(&mut self) -> Option<Affine2> {
        self.interaction_transform
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
    let state = OverlayState {
        name: format!("WayVR - {name}").into(),
        keyboard_focus: Some(KeyboardFocus::WayVR),
        want_visible: true,
        interactable: true,
        grabbable: true,
        spawn_scale: display_scale,
        spawn_point: vec3a(0.0, -0.1, -1.0),
        ..Default::default()
    };

    let wayvr = app.get_wayvr()?;

    let backend = Box::new(WayVRBackend::new(
        app,
        wayvr,
        display_handle,
        [display_width, display_height],
    )?);

    Ok(OverlayData {
        state,
        ..OverlayData::from_backend(backend)
    })
}

fn show_display<O>(wayvr: &mut WayVRData, overlays: &mut OverlayContainer<O>, display_name: &str)
where
    O: Default,
{
    if let Some(display) = WayVR::get_display_by_name(&wayvr.data.state.displays, display_name) {
        if let Some(overlay_id) = wayvr.display_handle_map.get(&display)
            && let Some(overlay) = overlays.mut_by_id(*overlay_id)
        {
            overlay.state.want_visible = true;
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
    let wayvr = app.get_wayvr()?;

    let catalog = app
        .session
        .wayvr_config
        .get_catalog(catalog_name)
        .ok_or_else(|| anyhow::anyhow!("Failed to get catalog \"{}\"", catalog_name))?
        .clone();

    if let Some(app_entry) = catalog.get_app(app_name) {
        let mut wayvr = wayvr.borrow_mut();

        let disp_handle = get_or_create_display_by_name(
            app,
            &mut wayvr,
            &app_entry.target_display.to_lowercase(),
        )?;

        let args_vec = &app_entry
            .args
            .as_ref()
            .map_or_else(Vec::new, |args| gen_args_vec(args.as_str()));

        let env_vec = &app_entry
            .env
            .as_ref()
            .map_or_else(Vec::new, |env| gen_env_vec(env));

        // Terminate existing process if required
        if let Some(process_handle) =
            wayvr
                .data
                .state
                .process_query(disp_handle, &app_entry.exec, args_vec, env_vec)
        {
            // Terminate process
            wayvr.data.terminate_process(process_handle);
        } else {
            // Spawn process
            wayvr.data.state.spawn_process(
                disp_handle,
                &app_entry.exec,
                args_vec,
                env_vec,
                None,
                HashMap::default(),
            )?;

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
            let wayvr = match app.get_wayvr() {
                Ok(wayvr) => wayvr,
                Err(e) => {
                    log::error!("WayVR Error: {e:?}");
                    return;
                }
            };

            let mut wayvr = wayvr.borrow_mut();

            if let Err(e) = toggle_dashboard::<O>(app, overlays, &mut wayvr) {
                error_toast(app, "toggle_dashboard failed", e);
            }
        }
    }
}
