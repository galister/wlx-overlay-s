use std::{f32::consts::PI, sync::Arc};

use backend::ScreenBackend;
use glam::{vec3a, Quat, Vec3};
use wl::create_screens_wayland;
use wlx_capture::frame::Transform;

use crate::{
    backend::overlay::{OverlayState, Positioning},
    state::{AppSession, AppState, ScreenMeta},
    subsystem::{hid::XkbKeymap, input::KeyboardFocus},
};

pub mod backend;
mod capture;
#[cfg(feature = "pipewire")]
pub mod pw;
#[cfg(feature = "wayland")]
pub mod wl;
#[cfg(feature = "x11")]
pub mod x11;

fn create_screen_state(name: Arc<str>, transform: Transform, session: &AppSession) -> OverlayState {
    let angle = if session.config.upright_screen_fix {
        match transform {
            Transform::Rotated90 | Transform::Flipped90 => PI / 2.,
            Transform::Rotated180 | Transform::Flipped180 => PI,
            Transform::Rotated270 | Transform::Flipped270 => -PI / 2.,
            _ => 0.,
        }
    } else {
        0.
    };

    OverlayState {
        name,
        keyboard_focus: Some(KeyboardFocus::PhysicalScreen),
        grabbable: true,
        recenter: true,
        positioning: Positioning::Anchored,
        interactable: true,
        spawn_scale: 1.5 * session.config.desktop_view_scale,
        spawn_point: vec3a(0., 0.5, 0.),
        spawn_rotation: Quat::from_axis_angle(Vec3::Z, angle),
        ..Default::default()
    }
}

pub struct ScreenCreateData {
    pub screens: Vec<(ScreenMeta, OverlayState, Box<ScreenBackend>)>,
}

pub fn create_screens(app: &mut AppState) -> anyhow::Result<(ScreenCreateData, Option<XkbKeymap>)> {
    app.screens.clear();

    #[cfg(feature = "wayland")]
    {
        if let Some(mut wl) = wlx_capture::wayland::WlxClient::new() {
            log::info!("Wayland detected.");
            let keymap = crate::subsystem::hid::get_keymap_wl()
                .map_err(|f| log::warn!("Could not load keyboard layout: {f}"))
                .ok();

            return Ok((create_screens_wayland(&mut wl, app), keymap));
        }
        log::info!("Wayland not detected, assuming X11.");
    }

    #[cfg(feature = "x11")]
    {
        let keymap = crate::subsystem::hid::get_keymap_x11()
            .map_err(|f| log::warn!("Could not load keyboard layout: {f}"))
            .ok();

        #[cfg(feature = "pipewire")]
        match x11::create_screens_x11pw(app) {
            Ok(data) => return Ok((data, keymap)),
            Err(e) => log::info!("Will not use X11 PipeWire capture: {e:?}"),
        }

        Ok((x11::create_screens_xshm(app)?, keymap))
    }
    #[cfg(not(feature = "x11"))]
    anyhow::bail!("No backends left to try.")
}
