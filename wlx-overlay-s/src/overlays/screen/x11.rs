use std::sync::Arc;

use glam::vec2;
use wlx_capture::{
    WlxCapture,
    frame::Transform,
    xshm::{XshmCapture, XshmScreen},
};

use crate::{
    overlays::screen::create_screen_from_backend,
    state::{AppState, ScreenMeta},
};

use super::{
    ScreenCreateData,
    backend::ScreenBackend,
    capture::{MainThreadWlxCapture, new_wlx_capture},
};

#[cfg(feature = "pipewire")]
use wlx_capture::pipewire::PipewireStream;

impl ScreenBackend {
    pub fn new_xshm(screen: Arc<XshmScreen>, app: &AppState) -> Self {
        let capture = new_wlx_capture!(
            app.gfx_extras.queue_capture,
            XshmCapture::new(screen.clone())
        );
        Self::new_raw(screen.name.clone(), capture)
    }
}

#[cfg(feature = "pipewire")]
pub fn create_screens_x11pw(app: &mut AppState) -> anyhow::Result<ScreenCreateData> {
    use glam::vec2;
    use wlx_capture::{pipewire::PipewireCapture, xshm::xshm_get_monitors};
    use wlx_common::{astr_containers::AStrMapExt, config::PwTokenMap};

    use crate::{
        overlays::screen::{
            create_screen_from_backend,
            pw::{load_pw_token_config, save_pw_token_config, select_pw_screen},
        },
        state::ScreenMeta,
    };

    use super::ScreenCreateData;

    // Load existing Pipewire tokens from file
    let mut pw_tokens: PwTokenMap = load_pw_token_config().unwrap_or_default();
    let pw_tokens_copy = pw_tokens.clone();
    let token = pw_tokens.arc_get("x11").map(std::string::String::as_str);
    let embed_mouse = !app.session.config.double_cursor_fix;

    let select_screen_result = select_pw_screen(
        app,
        "Select ALL screens on the screencast pop-up!",
        token,
        embed_mouse,
        true,
        true,
        true,
    )?;

    if let Some(restore_token) = select_screen_result.restore_token
        && pw_tokens.arc_set("x11".into(), restore_token.clone())
    {
        log::info!("Adding Pipewire token {restore_token}");
    }
    if pw_tokens_copy != pw_tokens {
        // Token list changed, re-create token config file
        if let Err(err) = save_pw_token_config(pw_tokens) {
            log::error!("Failed to save Pipewire token config: {err}");
        }
    }

    let monitors = match xshm_get_monitors() {
        Ok(m) => m,
        Err(e) => {
            anyhow::bail!(e.to_string());
        }
    };
    log::info!("Got {} monitors", monitors.len());
    log::info!("Got {} streams", select_screen_result.streams.len());

    let mut extent = vec2(0., 0.);
    let screens = select_screen_result
        .streams
        .into_iter()
        .enumerate()
        .map(|(i, s)| {
            let m = best_match(&s, monitors.iter().map(AsRef::as_ref)).unwrap();
            log::info!("Stream {i} is {}", m.name);
            extent.x = extent.x.max((m.monitor.x() + m.monitor.width()) as f32);
            extent.y = extent.y.max((m.monitor.y() + m.monitor.height()) as f32);

            let mut backend = ScreenBackend::new_raw(
                m.name.clone(),
                new_wlx_capture!(
                    app.gfx_extras.queue_capture,
                    PipewireCapture::new(m.name.clone(), s.node_id)
                ),
            );

            backend.set_mouse_transform(
                vec2(m.monitor.x() as f32, m.monitor.y() as f32),
                vec2(m.monitor.width() as f32, m.monitor.height() as f32),
                Transform::Normal,
            );

            let window_data = create_screen_from_backend(
                m.name.clone(),
                Transform::Normal,
                &app.session,
                Box::new(backend),
            );

            let meta = ScreenMeta {
                name: m.name.clone(),
                native_handle: 0,
            };

            (meta, window_data)
        })
        .collect();

    app.hid_provider.inner.set_desktop_extent(extent);
    app.hid_provider.inner.set_desktop_origin(vec2(0.0, 0.0));

    Ok(ScreenCreateData { screens })
}

#[cfg(feature = "pipewire")]
fn best_match<'a>(
    stream: &PipewireStream,
    mut streams: impl Iterator<Item = &'a XshmScreen>,
) -> Option<&'a XshmScreen> {
    let mut best = streams.next();
    log::debug!("stream: {:?}", stream.position);
    log::debug!("first: {:?}", best.map(|b| &b.monitor));
    let Some(position) = stream.position else {
        return best;
    };

    let mut best_dist = best.map_or(i32::MAX, |b| {
        (b.monitor.x() - position.0).abs() + (b.monitor.y() - position.1).abs()
    });
    for stream in streams {
        log::debug!("checking: {:?}", stream.monitor);
        let dist =
            (stream.monitor.x() - position.0).abs() + (stream.monitor.y() - position.1).abs();
        if dist < best_dist {
            best = Some(stream);
            best_dist = dist;
        }
    }
    log::debug!("best: {:?}", best.map(|b| &b.monitor));
    best
}

pub fn create_screens_xshm(app: &mut AppState) -> anyhow::Result<ScreenCreateData> {
    use wlx_capture::xshm::xshm_get_monitors;

    let mut extent = vec2(0., 0.);

    let monitors = match xshm_get_monitors() {
        Ok(m) => m,
        Err(e) => {
            anyhow::bail!(e.to_string());
        }
    };

    let screens = monitors
        .into_iter()
        .map(|s| {
            extent.x = extent.x.max((s.monitor.x() + s.monitor.width()) as f32);
            extent.y = extent.y.max((s.monitor.y() + s.monitor.height()) as f32);

            let size = (s.monitor.width(), s.monitor.height());
            let pos = (s.monitor.x(), s.monitor.y());
            let mut backend = ScreenBackend::new_xshm(s.clone(), app);

            log::info!(
                "{}: Init X11 screen of res {:?} at {:?}",
                s.name.clone(),
                size,
                pos,
            );

            backend.set_mouse_transform(
                vec2(s.monitor.x() as f32, s.monitor.y() as f32),
                vec2(size.0 as f32, size.1 as f32),
                Transform::Normal,
            );

            let window_data = create_screen_from_backend(
                s.name.clone(),
                Transform::Normal,
                &app.session,
                Box::new(backend),
            );

            let meta = ScreenMeta {
                name: s.name.clone(),
                native_handle: 0,
            };

            (meta, window_data)
        })
        .collect();

    app.hid_provider.inner.set_desktop_extent(extent);
    app.hid_provider.inner.set_desktop_origin(vec2(0.0, 0.0));

    Ok(ScreenCreateData { screens })
}
