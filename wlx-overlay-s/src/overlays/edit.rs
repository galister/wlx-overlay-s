use std::{
    any::Any,
    mem::{self, ManuallyDrop},
    sync::Arc,
};

use glam::{UVec2, vec2};

use crate::{
    backend::input::HoverResult,
    gui::panel::{GuiPanel, NewGuiPanelParams},
    state::AppState,
    subsystem::hid::WheelDelta,
    windowing::{
        backend::{DummyBackend, OverlayBackend, RenderResources, ShouldRender},
        window::OverlayWindowConfig,
    },
};

type EditModeWrapPanel = GuiPanel<Arc<str>>;

#[derive(Default)]
pub struct EditWrapperManager {
    edit_mode: bool,
    panel_pool: Vec<EditModeWrapPanel>,
}

impl EditWrapperManager {
    pub fn wrap_edit_mode(
        &mut self,
        owc: &mut OverlayWindowConfig,
        app: &mut AppState,
    ) -> anyhow::Result<()> {
        if owc.editing {
            return Ok(());
        }

        let Some(meta) = owc.backend.frame_meta() else {
            log::error!("META NULL");
            return Ok(());
        };

        log::debug!("EditMode wrap on {}", owc.name);
        let inner = mem::replace(&mut owc.backend, Box::new(DummyBackend {}));
        let mut panel = self.panel_pool.pop();
        if panel.is_none() {
            panel = Some(make_edit_panel(
                app,
                UVec2::new(meta.extent[0], meta.extent[1]),
            )?);
        }
        let mut panel = panel.unwrap();
        panel.state = owc.name.clone();
        owc.backend = Box::new(EditModeBackendWrapper {
            inner: ManuallyDrop::new(inner),
            panel: ManuallyDrop::new(panel),
        });
        owc.editing = true;

        Ok(())
    }

    pub fn unwrap_edit_mode(&mut self, owc: &mut OverlayWindowConfig) {
        if !owc.editing {
            return;
        }

        log::debug!("EditMode unwrap on {}", owc.name);
        let wrapper = mem::replace(&mut owc.backend, Box::new(DummyBackend {}));
        let mut wrapper: Box<dyn Any> = wrapper;
        let wrapper = wrapper
            .downcast_mut::<EditModeBackendWrapper>()
            .expect("Wrong type to unwrap");

        let panel = unsafe { ManuallyDrop::take(&mut wrapper.panel) };
        self.panel_pool.push(panel);

        let inner = unsafe { ManuallyDrop::take(&mut wrapper.inner) };
        owc.backend = inner;
        owc.editing = false;

        // wrapper is destroyed with nothing left inside
    }
}

pub struct EditModeBackendWrapper {
    panel: ManuallyDrop<EditModeWrapPanel>,
    inner: ManuallyDrop<Box<dyn OverlayBackend>>,
}

impl OverlayBackend for EditModeBackendWrapper {
    fn init(&mut self, app: &mut crate::state::AppState) -> anyhow::Result<()> {
        self.inner.init(app)?;
        self.panel.init(app)
    }
    fn pause(&mut self, app: &mut crate::state::AppState) -> anyhow::Result<()> {
        self.inner.pause(app)?;
        self.panel.pause(app)
    }
    fn resume(&mut self, app: &mut crate::state::AppState) -> anyhow::Result<()> {
        self.inner.resume(app)?;
        self.panel.resume(app)
    }
    fn should_render(&mut self, app: &mut crate::state::AppState) -> anyhow::Result<ShouldRender> {
        let i = self.inner.should_render(app)?;

        if !matches!(i, ShouldRender::Unable)
            && let Some(ref frame_meta) = self.inner.frame_meta()
        {
            let (width_px, height_px) = (frame_meta.extent[0], frame_meta.extent[1]);

            let new_size = vec2(width_px as _, height_px as _);
            if self.panel.max_size != new_size {
                log::debug!("EditWrapperGui size {} → {new_size}", self.panel.max_size);
                self.panel.max_size = new_size;

                let gui_scale = width_px.min(height_px) as f32 / 550.0;
                self.panel.gui_scale = gui_scale;
                self.panel.update_layout()?;
            }
        } else {
            return Ok(ShouldRender::Unable);
        }

        let p = self.panel.should_render(app)?;

        #[allow(clippy::match_same_arms)]
        Ok(match (i, p) {
            (ShouldRender::Should, ShouldRender::Should) => ShouldRender::Should,
            (ShouldRender::Should, ShouldRender::Can) => ShouldRender::Should,
            (ShouldRender::Can, ShouldRender::Should) => ShouldRender::Should,
            (ShouldRender::Can, ShouldRender::Can) => ShouldRender::Can,
            _ => ShouldRender::Unable,
        })
    }
    fn render(
        &mut self,
        app: &mut crate::state::AppState,
        rdr: &mut RenderResources,
    ) -> anyhow::Result<()> {
        self.inner.render(app, rdr)?;
        self.panel.render(app, rdr)
    }
    fn frame_meta(&mut self) -> Option<crate::windowing::backend::FrameMeta> {
        self.inner.frame_meta()
    }
    fn on_hover(
        &mut self,
        app: &mut crate::state::AppState,
        hit: &crate::backend::input::PointerHit,
    ) -> HoverResult {
        // pass through hover events to force pipewire to capture frames for us
        let _ = self.inner.on_hover(app, hit);
        self.panel.on_hover(app, hit)
    }
    fn on_left(&mut self, app: &mut crate::state::AppState, pointer: usize) {
        self.inner.on_left(app, pointer);
        self.panel.on_left(app, pointer);
    }
    fn on_pointer(
        &mut self,
        app: &mut crate::state::AppState,
        hit: &crate::backend::input::PointerHit,
        pressed: bool,
    ) {
        self.panel.on_pointer(app, hit, pressed);
    }
    fn on_scroll(
        &mut self,
        app: &mut crate::state::AppState,
        hit: &crate::backend::input::PointerHit,
        delta: WheelDelta,
    ) {
        self.panel.on_scroll(app, hit, delta);
    }
    fn get_interaction_transform(&mut self) -> Option<glam::Affine2> {
        self.inner.get_interaction_transform()
    }
}

fn make_edit_panel(
    app: &mut AppState,
    overlay_resolution: UVec2,
) -> anyhow::Result<EditModeWrapPanel> {
    log::error!(
        "overlay res {} {}",
        overlay_resolution.x,
        overlay_resolution.y
    );

    let panel = GuiPanel::new_from_template(
        app,
        "gui/edit.xml",
        "".into(),
        NewGuiPanelParams {
            resize_to_parent: true,
            ..Default::default()
        },
    )?;

    Ok(panel)
}
