use std::{
    f32::consts::PI,
    ops::Add,
    sync::{atomic::AtomicUsize, Arc},
    time::Instant,
};

use glam::{vec3a, Quat, Vec3A};
use idmap_derive::IntegerId;
use serde::{Deserialize, Serialize};

use crate::{
    backend::{
        common::{OverlaySelector, TaskType},
        overlay::{OverlayBackend, OverlayState, RelativeTo},
    },
    gui::{color_parse, CanvasBuilder},
    state::{AppState, LeftRight},
};

const FONT_SIZE: isize = 16;
const PADDING: (f32, f32) = (25., 7.);
const PIXELS_TO_METERS: f32 = 1. / 2000.;
const TOAST_AUDIO_WAV: &[u8] = include_bytes!("../res/557297.wav");

static AUTO_INCREMENT: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum DisplayMethod {
    Hide,
    Center,
    Watch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, IntegerId, Serialize, Deserialize)]
pub enum ToastTopic {
    System,
    DesktopNotification,
    XSNotification,
    IpdChange,
}

pub struct Toast {
    pub title: Arc<str>,
    pub body: Arc<str>,
    pub opacity: f32,
    pub timeout: f32,
    pub sound: bool,
    pub topic: ToastTopic,
}

#[allow(dead_code)]
impl Toast {
    pub fn new(topic: ToastTopic, title: Arc<str>, body: Arc<str>) -> Self {
        Toast {
            title,
            body,
            opacity: 1.0,
            timeout: 3.0,
            sound: false,
            topic,
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
        self.submit_at(app, Instant::now());
    }
    pub fn submit_at(self, app: &mut AppState, instant: Instant) {
        let auto_increment = AUTO_INCREMENT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let name: Arc<str> = format!("toast-{}", auto_increment).into();
        let selector = OverlaySelector::Name(name.clone());

        let destroy_at = instant.add(std::time::Duration::from_secs_f32(self.timeout));

        let has_sound = self.sound && app.session.config.notifications_sound_enabled;

        app.tasks.enqueue_at(
            TaskType::CreateOverlay(
                selector.clone(),
                Box::new(move |app| new_toast(self, name, app)),
            ),
            instant,
        );

        app.tasks
            .enqueue_at(TaskType::DropOverlay(selector), destroy_at);

        if has_sound {
            app.audio.play(TOAST_AUDIO_WAV);
        }
    }
}

fn new_toast(
    toast: Toast,
    name: Arc<str>,
    app: &mut AppState,
) -> Option<(OverlayState, Box<dyn OverlayBackend>)> {
    let current_method = app
        .session
        .toast_topics
        .get(toast.topic)
        .copied()
        .unwrap_or(DisplayMethod::Hide);

    let (spawn_point, spawn_rotation, relative_to) = match current_method {
        DisplayMethod::Hide => return None,
        DisplayMethod::Center => (vec3a(0., -0.2, -0.5), Quat::IDENTITY, RelativeTo::Head),
        DisplayMethod::Watch => {
            let mut watch_pos =
                Vec3A::from_slice(&app.session.config.watch_pos) + vec3a(-0.005, -0.05, 0.02);
            let mut watch_rot = Quat::from_slice(&app.session.config.watch_rot);
            let relative_to = match app.session.config.watch_hand {
                LeftRight::Left => RelativeTo::Hand(0),
                LeftRight::Right => {
                    watch_pos.x = -watch_pos.x;
                    watch_rot = watch_rot * Quat::from_rotation_x(PI) * Quat::from_rotation_z(PI);
                    RelativeTo::Hand(1)
                }
            };
            (watch_pos, watch_rot, relative_to)
        }
    };

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
        canvas.label(PADDING.0, 54., og_width, size.1 - 54., toast.body);

        canvas.fg_color = color_parse("#101010").unwrap(); // want panic
        canvas.bg_color = color_parse("#666666").unwrap(); // want panic
        canvas.panel(0., 0., size.0, 30.);
        canvas.label_centered(PADDING.0, 16., og_width, FONT_SIZE as f32 + 2., title);
    } else {
        canvas.label_centered(PADDING.0, 0., og_width, size.1, title);
    }

    let state = OverlayState {
        name,
        want_visible: true,
        spawn_scale: size.0 * PIXELS_TO_METERS,
        spawn_rotation,
        spawn_point,
        relative_to,
        ..Default::default()
    };
    let backend = Box::new(canvas.build());

    Some((state, backend))
}
