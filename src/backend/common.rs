use std::sync::Arc;

use once_cell::sync::Lazy;
#[cfg(feature = "openxr")]
use openxr as xr;

use glam::{Affine3A, Vec3, Vec3A};
use idmap::IdMap;
use serde::Deserialize;
use thiserror::Error;

use crate::{
    config::AStrSetExt,
    hid::{get_keymap_wl, get_keymap_x11},
    overlays::{
        anchor::create_anchor,
        keyboard::create_keyboard,
        screen::WlxClientAlias,
        watch::{create_watch, WATCH_NAME},
    },
    state::AppState,
};

use super::overlay::OverlayData;

#[derive(Error, Debug)]
pub enum BackendError {
    #[error("backend not supported")]
    NotSupported,
    #[cfg(feature = "openxr")]
    #[error("OpenXR Error: {0:?}")]
    OpenXrError(#[from] xr::sys::Result),
    #[error("Shutdown")]
    Shutdown,
    #[error("Restart")]
    Restart,
    #[error("Fatal: {0:?}")]
    Fatal(#[from] anyhow::Error),
}

#[cfg(feature = "wayland")]
fn create_wl_client() -> Option<WlxClientAlias> {
    wlx_capture::wayland::WlxClient::new()
}

#[cfg(not(feature = "wayland"))]
fn create_wl_client() -> Option<WlxClientAlias> {
    None
}

pub struct OverlayContainer<T>
where
    T: Default,
{
    overlays: IdMap<usize, OverlayData<T>>,
    wl: Option<WlxClientAlias>,
}

impl<T> OverlayContainer<T>
where
    T: Default,
{
    pub fn new(app: &mut AppState) -> anyhow::Result<Self> {
        let mut overlays = IdMap::new();
        let mut wl = create_wl_client();

        let keymap;

        app.screens.clear();
        let data = if let Some(wl) = wl.as_mut() {
            keymap = get_keymap_wl()
                .map_err(|f| log::warn!("Could not load keyboard layout: {}", f))
                .ok();
            crate::overlays::screen::create_screens_wayland(wl, app)?
        } else {
            keymap = get_keymap_x11()
                .map_err(|f| log::warn!("Could not load keyboard layout: {}", f))
                .ok();
            match crate::overlays::screen::create_screens_x11pw(app) {
                Ok(data) => data,
                Err(e) => {
                    log::info!("Will not use PipeWire capture: {:?}", e);
                    crate::overlays::screen::create_screens_xshm(app)?
                }
            }
        };

        let mut show_screens = app.session.config.show_screens.clone();
        if show_screens.is_empty() {
            if let Some((_, s, _)) = data.screens.first() {
                show_screens.arc_set(s.name.clone());
            }
        }

        for (meta, mut state, backend) in data.screens {
            if show_screens.arc_get(state.name.as_ref()) {
                state.show_hide = true;
            }
            overlays.insert(
                state.id,
                OverlayData::<T> {
                    state,
                    backend,
                    ..Default::default()
                },
            );
            app.screens.push(meta);
        }

        let anchor = create_anchor(app)?;
        overlays.insert(anchor.state.id, anchor);

        let mut watch = create_watch::<T>(app)?;
        watch.state.want_visible = true;
        overlays.insert(watch.state.id, watch);

        let mut keyboard = create_keyboard(app, keymap)?;
        keyboard.state.show_hide = true;
        keyboard.state.want_visible = false;
        overlays.insert(keyboard.state.id, keyboard);

        Ok(Self { overlays, wl })
    }

    #[cfg(not(feature = "wayland"))]
    pub fn update(&mut self, _app: &mut AppState) -> anyhow::Result<Vec<OverlayData<T>>> {
        Ok(vec![])
    }
    #[cfg(feature = "wayland")]
    pub fn update(&mut self, app: &mut AppState) -> anyhow::Result<Vec<OverlayData<T>>> {
        use crate::overlays::{
            screen::{create_screen_interaction, create_screen_renderer_wl, load_pw_token_config},
            watch::create_watch_canvas,
        };
        use glam::vec2;
        use wlx_capture::wayland::OutputChangeEvent;

        let mut removed_overlays = vec![];
        let Some(wl) = self.wl.as_mut() else {
            return Ok(removed_overlays);
        };

        wl.dispatch_pending();

        let mut create_ran = false;
        let mut extent_dirty = false;
        let mut watch_dirty = false;

        let mut maybe_token_store = None;

        for ev in wl.iter_events().collect::<Vec<_>>() {
            match ev {
                OutputChangeEvent::Create(_) => {
                    if create_ran {
                        continue;
                    }
                    let data = crate::overlays::screen::create_screens_wayland(wl, app)?;
                    create_ran = true;
                    for (meta, state, backend) in data.screens {
                        self.overlays.insert(
                            state.id,
                            OverlayData::<T> {
                                state,
                                backend,
                                ..Default::default()
                            },
                        );
                        app.screens.push(meta);
                        watch_dirty = true;
                    }
                }
                OutputChangeEvent::Destroy(id) => {
                    let Some(idx) = app.screens.iter().position(|s| s.native_handle == id) else {
                        continue;
                    };

                    let meta = &app.screens[idx];
                    let removed = self.overlays.remove(meta.id).unwrap();
                    removed_overlays.push(removed);
                    log::info!("{}: Destroyed", meta.name);
                    app.screens.remove(idx);
                    watch_dirty = true;
                    extent_dirty = true;
                }
                OutputChangeEvent::Logical(id) => {
                    let Some(meta) = app.screens.iter().find(|s| s.native_handle == id) else {
                        continue;
                    };
                    let output = wl.outputs.get(id).unwrap();
                    let Some(overlay) = self.overlays.get_mut(meta.id) else {
                        continue;
                    };
                    let logical_pos =
                        vec2(output.logical_pos.0 as f32, output.logical_pos.1 as f32);
                    let logical_size =
                        vec2(output.logical_size.0 as f32, output.logical_size.1 as f32);
                    let transform = output.transform.into();
                    overlay
                        .backend
                        .set_interaction(Box::new(create_screen_interaction(
                            logical_pos,
                            logical_size,
                            transform,
                        )));
                    extent_dirty = true;
                }
                OutputChangeEvent::Physical(id) => {
                    let Some(meta) = app.screens.iter().find(|s| s.native_handle == id) else {
                        continue;
                    };
                    let output = wl.outputs.get(id).unwrap();
                    let Some(overlay) = self.overlays.get_mut(meta.id) else {
                        continue;
                    };

                    let has_wlr_dmabuf = wl.maybe_wlr_dmabuf_mgr.is_some();
                    let has_wlr_screencopy = wl.maybe_wlr_screencopy_mgr.is_some();

                    let pw_token_store = maybe_token_store.get_or_insert_with(|| {
                        load_pw_token_config().unwrap_or_else(|e| {
                            log::warn!("Failed to load PipeWire token config: {:?}", e);
                            Default::default()
                        })
                    });

                    if let Some(renderer) = create_screen_renderer_wl(
                        output,
                        has_wlr_dmabuf,
                        has_wlr_screencopy,
                        pw_token_store,
                        &app.session,
                    ) {
                        overlay.backend.set_renderer(Box::new(renderer));
                    }
                    extent_dirty = true;
                }
            }
        }

        if extent_dirty && !create_ran {
            let extent = wl.get_desktop_extent();
            let origin = wl.get_desktop_origin();
            app.hid_provider
                .set_desktop_extent(vec2(extent.0 as f32, extent.1 as f32));
            app.hid_provider
                .set_desktop_origin(vec2(origin.0 as f32, origin.1 as f32));
        }

        if watch_dirty {
            let watch = self.mut_by_name(WATCH_NAME).unwrap(); // want panic
            match create_watch_canvas(None, app) {
                Ok(canvas) => {
                    watch.backend = Box::new(canvas);
                }
                Err(e) => {
                    log::error!("Failed to create watch canvas: {}", e);
                }
            }
        }

        Ok(removed_overlays)
    }

    pub fn mut_by_selector(&mut self, selector: &OverlaySelector) -> Option<&mut OverlayData<T>> {
        match selector {
            OverlaySelector::Id(id) => self.mut_by_id(*id),
            OverlaySelector::Name(name) => self.mut_by_name(name),
        }
    }

    pub fn remove_by_selector(&mut self, selector: &OverlaySelector) -> Option<OverlayData<T>> {
        match selector {
            OverlaySelector::Id(id) => self.overlays.remove(id),
            OverlaySelector::Name(name) => {
                let id = self
                    .overlays
                    .iter()
                    .find(|(_, o)| *o.state.name == **name)
                    .map(|(id, _)| *id);
                id.and_then(|id| self.overlays.remove(id))
            }
        }
    }

    pub fn get_by_id(&mut self, id: usize) -> Option<&OverlayData<T>> {
        self.overlays.get(id)
    }

    pub fn mut_by_id(&mut self, id: usize) -> Option<&mut OverlayData<T>> {
        self.overlays.get_mut(id)
    }

    pub fn get_by_name<'a>(&'a mut self, name: &str) -> Option<&'a OverlayData<T>> {
        self.overlays.values().find(|o| *o.state.name == *name)
    }

    pub fn mut_by_name<'a>(&'a mut self, name: &str) -> Option<&'a mut OverlayData<T>> {
        self.overlays.values_mut().find(|o| *o.state.name == *name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &'_ OverlayData<T>> {
        self.overlays.values()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &'_ mut OverlayData<T>> {
        self.overlays.values_mut()
    }

    pub fn add(&mut self, overlay: OverlayData<T>) {
        self.overlays.insert(overlay.state.id, overlay);
    }

    pub fn show_hide(&mut self, app: &mut AppState) {
        let any_shown = self
            .overlays
            .values()
            .any(|o| o.state.show_hide && o.state.want_visible);

        if !any_shown {
            static ANCHOR_LOCAL: Lazy<Affine3A> =
                Lazy::new(|| Affine3A::from_translation(Vec3::NEG_Z));
            let hmd = snap_upright(app.input_state.hmd, Vec3A::Y);
            app.anchor = hmd * *ANCHOR_LOCAL;
        }

        self.overlays.values_mut().for_each(|o| {
            if o.state.show_hide {
                o.state.want_visible = !any_shown;
                if o.state.want_visible
                    && app.session.config.realign_on_showhide
                    && o.state.recenter
                {
                    o.state.reset(app, false);
                }
            }
            // toggle watch back on if it was hidden
            if !any_shown && *o.state.name == *WATCH_NAME {
                o.state.reset(app, true);
            }
        })
    }
}

#[derive(Clone, Deserialize, Debug)]
#[serde(untagged)]
pub enum OverlaySelector {
    Id(usize),
    Name(Arc<str>),
}

pub fn snap_upright(transform: Affine3A, up_dir: Vec3A) -> Affine3A {
    if transform.x_axis.dot(up_dir).abs() < 0.2 {
        let scale = transform.x_axis.length();
        let col_z = transform.z_axis.normalize();
        let col_y = up_dir;
        let col_x = col_y.cross(col_z);
        let col_y = col_z.cross(col_x).normalize();
        let col_x = col_x.normalize();

        Affine3A::from_cols(
            col_x * scale,
            col_y * scale,
            col_z * scale,
            transform.translation,
        )
    } else {
        transform
    }
}
