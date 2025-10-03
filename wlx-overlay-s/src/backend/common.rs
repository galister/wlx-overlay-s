use std::sync::{Arc, LazyLock};

#[cfg(feature = "openxr")]
use openxr as xr;

use glam::{Affine3A, Vec3, Vec3A};
use slotmap::HopSlotMap;
use thiserror::Error;

use crate::{
    config::AStrSetExt,
    overlays::{
        anchor::create_anchor,
        keyboard::{builder::create_keyboard, KEYBOARD_NAME},
        screen::create_screens,
        watch::{create_watch, WATCH_NAME},
    },
    state::AppState,
};

use super::overlay::{OverlayData, OverlayID};

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

pub struct OverlayContainer<T>
where
    T: Default,
{
    overlays: HopSlotMap<OverlayID, OverlayData<T>>,
}

impl<T> OverlayContainer<T>
where
    T: Default,
{
    pub fn new(app: &mut AppState, headless: bool) -> anyhow::Result<Self> {
        let mut overlays = HopSlotMap::with_key();
        let mut show_screens = app.session.config.show_screens.clone();
        let mut maybe_keymap = None;

        if headless {
            log::info!("Running in headless mode; keyboard will be en-US");
        } else {
            match create_screens(app) {
                Ok((data, keymap)) => {
                    if show_screens.is_empty()
                        && let Some((_, s, _)) = data.screens.first()
                    {
                        show_screens.arc_set(s.name.clone());
                    }
                    for (meta, mut state, backend) in data.screens {
                        if show_screens.arc_get(state.name.as_ref()) {
                            state.show_hide = true;
                        }
                        overlays.insert(OverlayData::<T> {
                            state,
                            ..OverlayData::from_backend(backend)
                        });
                        app.screens.push(meta);
                    }

                    maybe_keymap = keymap;
                }
                Err(e) => log::error!("Unable to initialize screens: {e:?}"),
            }
        }

        let anchor = create_anchor(app)?;
        overlays.insert(anchor);

        let mut watch = create_watch::<T>(app)?;
        watch.state.want_visible = true;
        overlays.insert(watch);

        let mut keyboard = create_keyboard(app, maybe_keymap)?;
        keyboard.state.show_hide = show_screens.arc_get(KEYBOARD_NAME);
        keyboard.state.want_visible = false;
        overlays.insert(keyboard);

        Ok(Self { overlays })
    }

    pub fn mut_by_selector(&mut self, selector: &OverlaySelector) -> Option<&mut OverlayData<T>> {
        match selector {
            OverlaySelector::Id(id) => self.mut_by_id(*id),
            OverlaySelector::Name(name) => self.mut_by_name(name),
        }
    }

    pub fn remove_by_selector(&mut self, selector: &OverlaySelector) -> Option<OverlayData<T>> {
        match selector {
            OverlaySelector::Id(id) => self.overlays.remove(*id),
            OverlaySelector::Name(name) => {
                self.lookup(name).and_then(|id| self.overlays.remove(id))
            }
        }
    }

    pub fn get_by_id(&mut self, id: OverlayID) -> Option<&OverlayData<T>> {
        self.overlays.get(id)
    }

    pub fn mut_by_id(&mut self, id: OverlayID) -> Option<&mut OverlayData<T>> {
        self.overlays.get_mut(id)
    }

    pub fn get_by_name<'a>(&'a mut self, name: &str) -> Option<&'a OverlayData<T>> {
        self.overlays.values().find(|o| *o.state.name == *name)
    }

    pub fn mut_by_name<'a>(&'a mut self, name: &str) -> Option<&'a mut OverlayData<T>> {
        self.overlays.values_mut().find(|o| *o.state.name == *name)
    }

    pub fn iter(&self) -> impl Iterator<Item = (OverlayID, &'_ OverlayData<T>)> {
        self.overlays.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (OverlayID, &'_ mut OverlayData<T>)> {
        self.overlays.iter_mut()
    }

    pub fn values(&self) -> impl Iterator<Item = &'_ OverlayData<T>> {
        self.overlays.values()
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &'_ mut OverlayData<T>> {
        self.overlays.values_mut()
    }

    pub fn lookup(&self, name: &str) -> Option<OverlayID> {
        self.overlays
            .iter()
            .find(|(_, v)| v.state.name.as_ref() == name)
            .map(|(k, _)| k)
    }

    pub fn add(&mut self, overlay: OverlayData<T>) -> OverlayID {
        self.overlays.insert(overlay)
    }

    pub fn show_hide(&mut self, app: &mut AppState) {
        let any_shown = self
            .overlays
            .values()
            .any(|o| o.state.show_hide && o.state.want_visible);

        if !any_shown {
            static ANCHOR_LOCAL: LazyLock<Affine3A> =
                LazyLock::new(|| Affine3A::from_translation(Vec3::NEG_Z));
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
        });
    }
}

#[derive(Clone, Debug)]
pub enum OverlaySelector {
    Id(OverlayID),
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
