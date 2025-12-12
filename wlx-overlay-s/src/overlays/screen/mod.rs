use std::{f32::consts::PI, sync::Arc};

use glam::{Affine3A, Quat, Vec3, vec3};
use wlx_capture::frame::Transform;
use wlx_common::windowing::{OverlayWindowState, Positioning};

use crate::{
    state::{AppSession, AppState, ScreenMeta},
    subsystem::{hid::XkbKeymap, input::KeyboardFocus},
    windowing::{
        backend::OverlayBackend,
        window::{OverlayCategory, OverlayWindowConfig},
    },
};

pub mod backend;
mod capture;
#[cfg(feature = "pipewire")]
pub mod pw;
#[cfg(feature = "wayland")]
pub mod wl;
#[cfg(feature = "x11")]
pub mod x11;

fn create_screen_from_backend(
    name: Arc<str>,
    transform: Transform,
    session: &AppSession,
    backend: Box<dyn OverlayBackend>,
) -> OverlayWindowConfig {
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

    OverlayWindowConfig {
        name,
        category: OverlayCategory::Screen,
        default_state: OverlayWindowState {
            grabbable: true,
            positioning: Positioning::Anchored,
            interactable: true,
            curvature: Some(0.15),
            transform: Affine3A::from_scale_rotation_translation(
                Vec3::ONE * 1.5 * session.config.desktop_view_scale,
                Quat::from_rotation_z(angle),
                vec3(0.0, 0.2, -0.5),
            ),
            ..OverlayWindowState::default()
        },
        keyboard_focus: Some(KeyboardFocus::PhysicalScreen),
        ..OverlayWindowConfig::from_backend(backend)
    }
}

pub struct ScreenCreateData {
    pub screens: Vec<(ScreenMeta, OverlayWindowConfig)>,
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

            return Ok((wl::create_screens_wayland(&mut wl, app), keymap));
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
