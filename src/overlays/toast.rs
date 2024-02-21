use std::{
    io::Cursor,
    ops::Add,
    sync::{atomic::AtomicUsize, Arc},
};

use rodio::{Decoder, Source};

use glam::vec3a;

use crate::{
    backend::{
        common::{OverlaySelector, TaskType},
        overlay::{OverlayBackend, OverlayState, RelativeTo},
    },
    gui::{color_parse, CanvasBuilder},
    state::AppState,
};

const FONT_SIZE: isize = 16;
const PADDING: (f32, f32) = (25., 7.);
const PIXELS_TO_METERS: f32 = 1. / 2000.;

static AUTO_INCREMENT: AtomicUsize = AtomicUsize::new(0);

pub struct Toast {
    pub title: Arc<str>,
    pub body: Arc<str>,
    pub opacity: f32,
    pub timeout: f32,
    pub sound: bool,
}

#[allow(dead_code)]
impl Toast {
    pub fn new(title: Arc<str>, body: Arc<str>) -> Self {
        Toast {
            title,
            body,
            opacity: 1.0,
            timeout: 3.0,
            sound: false,
        }
    }
    pub fn with_timeout(mut self, timeout: f32) -> Self {
        self.timeout = timeout;
        self
    }
    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }
    pub fn with_sound(mut self, sound: bool) -> Self {
        self.sound = sound;
        self
    }
    pub fn submit(self, app: &mut AppState) {
        let auto_increment = AUTO_INCREMENT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let name: Arc<str> = format!("toast-{}", auto_increment).into();
        let selector = OverlaySelector::Name(name.clone());

        let destroy_at =
            std::time::Instant::now().add(std::time::Duration::from_secs_f32(self.timeout));

        let has_sound = self.sound;

        app.tasks.enqueue(TaskType::CreateOverlay(
            selector.clone(),
            Box::new(move |app| new_toast(self, name, app)),
        ));

        app.tasks
            .enqueue_at(TaskType::DropOverlay(selector), destroy_at);

        if has_sound {
            if let Some(handle) = app.audio.get_handle() {
                let wav = include_bytes!("../res/557297.wav");
                let cursor = Cursor::new(wav);
                let source = Decoder::new_wav(cursor).unwrap();
                let _ = handle.play_raw(source.convert_samples());
            }
        }
    }
}

fn new_toast(
    toast: Toast,
    name: Arc<str>,
    app: &mut AppState,
) -> Option<(OverlayState, Box<dyn OverlayBackend>)> {
    let title = if toast.title.len() > 0 {
        toast.title
    } else {
        "Notification".into()
    };

    let mut size = if toast.body.len() > 0 {
        let (w0, _) = app
            .fc
            .get_text_size(&title, FONT_SIZE, app.graphics.clone())
            .ok()?;
        let (w1, h1) = app
            .fc
            .get_text_size(&toast.body, FONT_SIZE, app.graphics.clone())
            .ok()?;
        (w0.max(w1), h1 + 50.)
    } else {
        let (w, h) = app
            .fc
            .get_text_size(&title, FONT_SIZE, app.graphics.clone())
            .ok()?;
        (w, h + 20.)
    };

    let og_width = size.0;
    size.0 += PADDING.0 * 2.;

    let mut canvas = CanvasBuilder::<(), ()>::new(
        size.0 as _,
        size.1 as _,
        app.graphics.clone(),
        app.graphics.native_format,
        (),
    )
    .ok()?;

    canvas.font_size = FONT_SIZE;
    canvas.fg_color = color_parse("#aaaaaa").unwrap(); // want panic
    canvas.bg_color = color_parse("#333333").unwrap(); // want panic
    canvas.panel(0., 0., size.0, size.1);

    if toast.body.len() > 0 {
        log::info!("{}: {}", title, toast.body);
        canvas.label(PADDING.0, 54., og_width, size.1 - 54., toast.body);

        canvas.fg_color = color_parse("#101010").unwrap(); // want panic
        canvas.bg_color = color_parse("#666666").unwrap(); // want panic
        canvas.panel(0., 0., size.0, 30.);
        canvas.label_centered(PADDING.0, 16., og_width, FONT_SIZE as f32 + 2., title);
    } else {
        log::info!("Toast: {}", title);
        canvas.label_centered(PADDING.0, 0., og_width, size.1, title);
    }

    let state = OverlayState {
        name,
        want_visible: true,
        spawn_scale: size.0 * PIXELS_TO_METERS,
        spawn_point: vec3a(0., -0.2, -0.5),
        relative_to: RelativeTo::Head,
        ..Default::default()
    };
    let backend = Box::new(canvas.build());

    Some((state, backend))
}
