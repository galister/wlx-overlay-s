use std::{
    any::Any,
    mem::{self, ManuallyDrop},
    sync::Arc,
};

use glam::vec2;
use vulkano::image::{view::ImageView, ImageUsage};

use crate::{
    backend::input::HoverResult,
    gui::panel::GuiPanel,
    state::AppState,
    windowing::{
        backend::{DummyBackend, OverlayBackend, ShouldRender},
        window::OverlayWindowConfig,
    },
};

type EditModeWrapPanel = GuiPanel<Arc<str>>;

#[derive(Default)]
pub struct EditModeManager {
    panel_pool: Vec<EditModeWrapPanel>,
}

impl EditModeManager {
    pub fn wrap_edit_mode(
        &mut self,
        owc: &mut OverlayWindowConfig,
        app: &mut AppState,
    ) -> anyhow::Result<()> {
        if owc.editing {
            return Ok(());
        }

        log::debug!("EditMode wrap on {}", owc.name);
        let inner = mem::replace(&mut owc.backend, Box::new(DummyBackend {}));
        let mut panel = self.panel_pool.pop();
        if panel.is_none() {
            panel = Some(make_adjustment_panel(app)?);
        }
        let mut panel = panel.unwrap();
        panel.state = owc.name.clone();
        owc.backend = Box::new(EditModeBackendWrapper {
            inner: ManuallyDrop::new(inner),
            panel: ManuallyDrop::new(panel),
            can_render_inner: false,
            image: None,
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
    image: Option<Arc<ImageView>>,
    can_render_inner: bool,
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

        self.can_render_inner = !matches!(i, ShouldRender::Unable);
        if self.can_render_inner
            && let Some(ref frame_meta) = self.inner.frame_meta()
        {
            let new_size = vec2(frame_meta.extent[0] as _, frame_meta.extent[1] as _);
            if self.panel.max_size != new_size {
                log::debug!("EditWrapperGui size {} → {new_size}", self.panel.max_size);
                self.panel.max_size = new_size;
                self.panel.update_layout()?;
            }
        }

        let p = self.panel.should_render(app)?;

        Ok(match (i, p) {
            (ShouldRender::Should, ShouldRender::Should) => ShouldRender::Should,
            (ShouldRender::Should, ShouldRender::Can) => ShouldRender::Should,
            (ShouldRender::Can, ShouldRender::Should) => ShouldRender::Should,
            (ShouldRender::Can, ShouldRender::Can) => ShouldRender::Can,
            // (ShouldRender::Unable, ShouldRender::Should) if self.image.is_some() => {
            //     ShouldRender::Should
            // }
            // (ShouldRender::Unable, ShouldRender::Can) if self.image.is_some() => ShouldRender::Can,
            _ => ShouldRender::Unable,
        })
    }
    fn render(
        &mut self,
        app: &mut crate::state::AppState,
        tgt: std::sync::Arc<vulkano::image::view::ImageView>,
        buf: &mut crate::graphics::CommandBuffers,
        alpha: f32,
    ) -> anyhow::Result<bool> {
        if self.can_render_inner {
            if self.image.is_none()
                && let Some(ref meta) = self.inner.frame_meta()
            {
                let image = app.gfx.new_image(
                    meta.extent[0],
                    meta.extent[1],
                    app.gfx.surface_format,
                    ImageUsage::COLOR_ATTACHMENT | ImageUsage::SAMPLED,
                )?;
                self.image = Some(ImageView::new_default(image)?);
            }
            self.inner.render(app, tgt.clone(), buf, alpha)?;
        }

        self.panel.render(app, tgt, buf, -1.)
    }
    fn frame_meta(&mut self) -> Option<crate::windowing::backend::FrameMeta> {
        self.inner.frame_meta().or_else(|| self.panel.frame_meta())
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
        delta_y: f32,
        delta_x: f32,
    ) {
        self.panel.on_scroll(app, hit, delta_y, delta_x);
    }
    fn get_interaction_transform(&mut self) -> Option<glam::Affine2> {
        self.inner.get_interaction_transform()
    }
}

fn make_adjustment_panel(app: &mut AppState) -> anyhow::Result<EditModeWrapPanel> {
    let mut panel = GuiPanel::new_from_template(app, "gui/adjust.xml", "".into(), None, true)?;
    panel.update_layout()?;

    Ok(panel)
}
