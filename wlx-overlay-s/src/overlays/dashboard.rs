use dash_frontend::{
    frontend::{self, FrontendUpdateParams},
    settings::{self, SettingsIO},
};
use glam::{Affine2, Affine3A, Vec2, vec2, vec3};
use wayvr_ipc::{
    packet_client::WvrProcessLaunchParams,
    packet_server::{WvrProcess, WvrProcessHandle, WvrWindow, WvrWindowHandle},
};
use wgui::{
    event::{
        Event as WguiEvent, MouseButtonIndex, MouseDownEvent, MouseLeaveEvent, MouseMotionEvent,
        MouseUpEvent, MouseWheelEvent,
    },
    gfx::cmd::WGfxClearMode,
    renderer_vk::context::Context as WguiContext,
    widget::EventResult,
};
use wlx_common::{
    dash_interface::DashInterface,
    overlays::{BackendAttrib, BackendAttribValue},
};
use wlx_common::{
    timestep::Timestep,
    windowing::{OverlayWindowState, Positioning},
};

use crate::{
    app_misc,
    backend::{
        input::{Haptics, HoverResult, PointerHit, PointerMode},
        task::{OverlayTask, PlayspaceTask, TaskType},
        wayvr::{process::ProcessHandle, window::WindowHandle},
    },
    ipc::ipc_server::{gen_args_vec, gen_env_vec},
    state::AppState,
    subsystem::hid::WheelDelta,
    windowing::{
        OverlaySelector, Z_ORDER_DASHBOARD,
        backend::{
            FrameMeta, OverlayBackend, OverlayEventData, RenderResources, ShouldRender,
            ui_transform,
        },
        window::{OverlayCategory, OverlayWindowConfig},
    },
};

pub const DASH_NAME: &str = "Dashboard";

const DASH_RES_U32A: [u32; 2] = [1920, 1080];
const DASH_RES_VEC2: Vec2 = vec2(DASH_RES_U32A[0] as _, DASH_RES_U32A[1] as _);

//FIXME: replace with proper impl
struct SimpleSettingsIO {
    settings: settings::Settings,
}
impl SimpleSettingsIO {
    fn new() -> Self {
        let mut res = Self {
            settings: settings::Settings::default(),
        };
        res.read_from_disk();
        res
    }
}
impl settings::SettingsIO for SimpleSettingsIO {
    fn get_mut(&mut self) -> &mut settings::Settings {
        &mut self.settings
    }
    fn get(&self) -> &dash_frontend::settings::Settings {
        &self.settings
    }
    fn save_to_disk(&mut self) {
        log::info!("saving settings");
        let data = self.settings.save();
        std::fs::write("/tmp/testbed_settings.json", data).unwrap();
    }
    fn read_from_disk(&mut self) {
        log::info!("loading settings");
        if let Ok(res) = std::fs::read("/tmp/testbed_settings.json") {
            let data = String::from_utf8(res).unwrap();
            self.settings = settings::Settings::load(&data).unwrap();
        }
    }
    fn mark_as_dirty(&mut self) {
        self.save_to_disk();
    }
}

pub struct DashFrontend {
    inner: frontend::Frontend<AppState>,
    initialized: bool,
    interaction_transform: Option<Affine2>,
    timestep: Timestep,
    has_focus: [bool; 2],
    context: WguiContext,
}

const GUI_SCALE: f32 = 2.0;

impl DashFrontend {
    fn new(app: &mut AppState) -> anyhow::Result<Self> {
        let settings = SimpleSettingsIO::new();
        let interface = DashInterfaceLive::new();

        let mut frontend = frontend::Frontend::new(frontend::InitParams {
            settings: Box::new(settings),
            interface: Box::new(interface),
        })?;

        frontend.play_startup_sound(&mut app.audio_system)?;

        let context = WguiContext::new(&mut app.wgui_shared, 1.0)?;
        Ok(Self {
            inner: frontend,
            initialized: false,
            interaction_transform: None,
            timestep: Timestep::new(60.0),
            has_focus: [false, false],
            context,
        })
    }

    fn update(&mut self, app: &mut AppState, timestep_alpha: f32) -> anyhow::Result<()> {
        let res = self.inner.update(FrontendUpdateParams {
            data: app,
            width: DASH_RES_VEC2.x / GUI_SCALE,
            height: DASH_RES_VEC2.y / GUI_SCALE,
            timestep_alpha,
        })?;
        app_misc::process_layout_result(app, res);
        Ok(())
    }

    fn push_event(&mut self, event: &WguiEvent) -> EventResult {
        match self.inner.layout.push_event(event, &mut (), &mut ()) {
            Ok(r) => r,
            Err(e) => {
                log::error!("Failed to push event: {e:?}");
                EventResult::NoHit
            }
        }
    }
}

impl OverlayBackend for DashFrontend {
    fn init(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        self.context
            .update_viewport(&mut app.wgui_shared, DASH_RES_U32A, GUI_SCALE)?;
        self.interaction_transform = Some(ui_transform(DASH_RES_U32A));

        if self.inner.layout.content_size.x * self.inner.layout.content_size.y != 0.0 {
            self.update(app, 0.0)?;
            self.initialized = true;
        }
        Ok(())
    }

    fn pause(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        Ok(())
    }

    fn resume(&mut self, _app: &mut AppState) -> anyhow::Result<()> {
        self.inner.layout.needs_redraw = true;
        self.timestep.reset();
        Ok(())
    }

    fn should_render(&mut self, app: &mut AppState) -> anyhow::Result<ShouldRender> {
        while self.timestep.on_tick() {
            self.inner.layout.tick()?;
        }

        if let Err(e) = self.update(app, self.timestep.alpha) {
            log::error!("uncaught exception: {e:?}");
        }

        Ok(if self.inner.layout.check_toggle_needs_redraw() {
            ShouldRender::Should
        } else {
            ShouldRender::Can
        })
    }

    fn render(&mut self, app: &mut AppState, rdr: &mut RenderResources) -> anyhow::Result<()> {
        let globals = self.inner.layout.state.globals.clone(); // sorry
        let mut globals = globals.get();

        let primitives = wgui::drawing::draw(&mut wgui::drawing::DrawParams {
            globals: &mut globals,
            layout: &mut self.inner.layout,
            debug_draw: false,
            timestep_alpha: self.timestep.alpha,
        })?;
        self.context.draw(
            &globals.font_system,
            &mut app.wgui_shared,
            rdr.cmd_buf_single(),
            &primitives,
        )?;
        Ok(())
    }

    fn frame_meta(&mut self) -> Option<FrameMeta> {
        Some(FrameMeta {
            clear: WGfxClearMode::Clear([0., 0., 0., 0.]),
            extent: [DASH_RES_U32A[0], DASH_RES_U32A[1], 1],
            ..Default::default()
        })
    }

    fn notify(&mut self, _app: &mut AppState, _data: OverlayEventData) -> anyhow::Result<()> {
        Ok(())
    }

    fn on_scroll(&mut self, _app: &mut AppState, hit: &PointerHit, delta: WheelDelta) {
        let e = WguiEvent::MouseWheel(MouseWheelEvent {
            delta: vec2(delta.x, delta.y),
            pos: hit.uv * self.inner.layout.content_size,
            device: hit.pointer,
        });
        self.push_event(&e);
    }

    fn on_hover(&mut self, _app: &mut AppState, hit: &PointerHit) -> HoverResult {
        let e = &WguiEvent::MouseMotion(MouseMotionEvent {
            pos: hit.uv * self.inner.layout.content_size,
            device: hit.pointer,
        });

        self.has_focus[hit.pointer] = true;

        let result = self.push_event(e);

        HoverResult {
            consume: result != EventResult::NoHit,
            haptics: self
                .inner
                .layout
                .check_toggle_haptics_triggered()
                .then_some(Haptics {
                    intensity: 0.1,
                    duration: 0.01,
                    frequency: 5.0,
                }),
        }
    }

    fn on_left(&mut self, _app: &mut AppState, pointer: usize) {
        let e = WguiEvent::MouseLeave(MouseLeaveEvent { device: pointer });
        self.has_focus[pointer] = false;
        self.push_event(&e);
    }

    fn on_pointer(&mut self, _app: &mut AppState, hit: &PointerHit, pressed: bool) {
        let index = match hit.mode {
            PointerMode::Left => MouseButtonIndex::Left,
            PointerMode::Right => MouseButtonIndex::Right,
            PointerMode::Middle => MouseButtonIndex::Middle,
            _ => return,
        };

        let e = if pressed {
            WguiEvent::MouseDown(MouseDownEvent {
                pos: hit.uv * self.inner.layout.content_size,
                index,
                device: hit.pointer,
            })
        } else {
            WguiEvent::MouseUp(MouseUpEvent {
                pos: hit.uv * self.inner.layout.content_size,
                index,
                device: hit.pointer,
            })
        };
        self.push_event(&e);

        // released while off-panel → send mouse leave as well
        if !pressed && !self.has_focus[hit.pointer] {
            let e = WguiEvent::MouseMotion(MouseMotionEvent {
                pos: vec2(-1., -1.),
                device: hit.pointer,
            });
            self.push_event(&e);
            let e = WguiEvent::MouseLeave(MouseLeaveEvent {
                device: hit.pointer,
            });
            self.push_event(&e);
        }
    }

    fn get_interaction_transform(&mut self) -> Option<Affine2> {
        self.interaction_transform
    }
    fn get_attrib(&self, _attrib: BackendAttrib) -> Option<BackendAttribValue> {
        None
    }
    fn set_attrib(&mut self, _app: &mut AppState, _value: BackendAttribValue) -> bool {
        false
    }
}

pub fn create_dash_frontend(app: &mut AppState) -> anyhow::Result<OverlayWindowConfig> {
    Ok(OverlayWindowConfig {
        name: DASH_NAME.into(),
        default_state: OverlayWindowState {
            transform: Affine3A::from_translation(vec3(0., 0., -0.9)),
            grabbable: true,
            interactable: true,
            positioning: Positioning::Floating,
            curvature: Some(0.15),
            ..OverlayWindowState::default()
        },
        z_order: Z_ORDER_DASHBOARD,
        category: OverlayCategory::Dashboard,
        global: true,
        ..OverlayWindowConfig::from_backend(Box::new(DashFrontend::new(app)?))
    })
}
pub struct DashInterfaceLive {}

impl DashInterfaceLive {
    pub fn new() -> Self {
        Self {}
    }
}

impl DashInterface<AppState> for DashInterfaceLive {
    fn window_list(&mut self, app: &mut AppState) -> anyhow::Result<Vec<WvrWindow>> {
        let wvr_server = app.wvr_server.as_mut().unwrap();
        Ok(wvr_server
            .wm
            .windows
            .iter()
            .map(|(handle, win)| WvrWindow {
                handle: WindowHandle::as_packet(&handle),
                process_handle: ProcessHandle::as_packet(&win.process),
                size_x: win.size_x,
                size_y: win.size_y,
                visible: win.visible,
            })
            .collect())
    }

    fn window_request_close(
        &mut self,
        app: &mut AppState,
        handle: WvrWindowHandle,
    ) -> anyhow::Result<()> {
        let wvr_server = app.wvr_server.as_mut().unwrap();
        wvr_server
            .wm
            .remove_window(WindowHandle::from_packet(handle));
        Ok(())
    }

    fn process_get(&mut self, app: &mut AppState, handle: WvrProcessHandle) -> Option<WvrProcess> {
        let wvr_server = app.wvr_server.as_mut().unwrap();
        let handle = ProcessHandle::from_packet(handle);
        wvr_server
            .processes
            .get(&handle)
            .map(|x| x.to_packet(handle))
    }

    fn process_launch(
        &mut self,
        app: &mut AppState,
        params: WvrProcessLaunchParams,
    ) -> anyhow::Result<WvrProcessHandle> {
        let wvr_server = app.wvr_server.as_mut().unwrap();

        let args_vec = gen_args_vec(&params.args);
        let env_vec = gen_env_vec(&params.env);

        wvr_server
            .spawn_process(
                &params.name,
                &params.exec,
                &args_vec,
                &env_vec,
                params.resolution,
                None,
                params.icon.as_deref(),
                params.userdata,
            )
            .map(|x| x.as_packet())
    }

    fn process_list(&mut self, app: &mut AppState) -> anyhow::Result<Vec<WvrProcess>> {
        let wvr_server = app.wvr_server.as_mut().unwrap();
        Ok(wvr_server
            .processes
            .iter()
            .map(|(hnd, p)| p.to_packet(hnd))
            .collect())
    }

    fn process_terminate(
        &mut self,
        app: &mut AppState,
        handle: WvrProcessHandle,
    ) -> anyhow::Result<()> {
        let wvr_server = app.wvr_server.as_mut().unwrap();
        wvr_server.terminate_process(ProcessHandle::from_packet(handle));
        Ok(())
    }

    fn window_set_visible(
        &mut self,
        app: &mut AppState,
        handle: WvrWindowHandle,
        visible: bool,
    ) -> anyhow::Result<()> {
        let wvr_server = app.wvr_server.as_mut().unwrap();
        let Some(oid) = wvr_server.get_overlay_id(WindowHandle::from_packet(handle)) else {
            return Ok(());
        };

        app.tasks.enqueue(TaskType::Overlay(OverlayTask::Modify(
            OverlaySelector::Id(oid),
            Box::new(move |app, owc| {
                if visible && !owc.is_active() {
                    owc.activate(app);
                } else if !visible && owc.is_active() {
                    owc.deactivate();
                }
            }),
        )));
        Ok(())
    }

    fn recenter_playspace(&mut self, app: &mut AppState) -> anyhow::Result<()> {
        app.tasks
            .enqueue(TaskType::Playspace(PlayspaceTask::Recenter));
        Ok(())
    }

    fn desktop_finder<'a>(
        &'a mut self,
        data: &'a mut AppState,
    ) -> &'a mut wlx_common::desktop_finder::DesktopFinder {
        &mut data.desktop_finder
    }
}
