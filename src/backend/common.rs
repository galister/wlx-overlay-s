use std::{
    collections::{BinaryHeap, VecDeque},
    sync::Arc,
    time::Instant,
};

#[cfg(feature = "openxr")]
use openxr as xr;

use glam::{Affine3A, Vec2, Vec3A};
use idmap::IdMap;
use thiserror::Error;

use crate::{
    overlays::{
        keyboard::create_keyboard,
        toast::Toast,
        watch::{create_watch, WATCH_NAME, WATCH_SCALE},
    },
    state::AppState,
};

use super::overlay::{OverlayBackend, OverlayData, OverlayState};

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
    overlays: IdMap<usize, OverlayData<T>>,
    pub extent: Vec2,
}

impl<T> OverlayContainer<T>
where
    T: Default,
{
    pub fn new(app: &mut AppState) -> anyhow::Result<Self> {
        let mut overlays = IdMap::new();
        let (screens, extent) = if std::env::var("WAYLAND_DISPLAY").is_ok() {
            crate::overlays::screen::get_screens_wayland(&app.session)?
        } else {
            crate::overlays::screen::get_screens_x11(&app.session)?
        };

        let mut watch = create_watch::<T>(app, &screens)?;
        watch.state.want_visible = true;
        overlays.insert(watch.state.id, watch);

        let mut keyboard = create_keyboard(app)?;
        keyboard.state.show_hide = true;
        keyboard.state.want_visible = false;
        overlays.insert(keyboard.state.id, keyboard);

        let mut show_screens = app.session.config.show_screens.clone();
        if show_screens.is_empty() {
            screens.first().map(|s| {
                show_screens.push(s.state.name.clone());
            });
        }

        for mut screen in screens {
            if show_screens.contains(&screen.state.name) {
                screen.state.show_hide = true;
                screen.state.want_visible = false;
            }
            overlays.insert(screen.state.id, screen);
        }
        Ok(Self { overlays, extent })
    }

    pub fn mut_by_selector(&mut self, selector: &OverlaySelector) -> Option<&mut OverlayData<T>> {
        match selector {
            OverlaySelector::Id(id) => self.mut_by_id(*id),
            OverlaySelector::Name(name) => self.mut_by_name(name),
        }
    }

    pub fn drop_by_selector(&mut self, selector: &OverlaySelector) {
        match selector {
            OverlaySelector::Id(id) => {
                self.overlays.remove(id);
            }
            OverlaySelector::Name(name) => {
                let id = self
                    .overlays
                    .iter()
                    .find(|(_, o)| *o.state.name == **name)
                    .map(|(id, _)| *id);
                id.and_then(|id| self.overlays.remove(id));
            }
        };
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

        self.overlays.values_mut().for_each(|o| {
            if o.state.show_hide {
                o.state.want_visible = !any_shown;
                if o.state.want_visible && o.state.recenter {
                    o.state.reset(app, false);
                }
            }
            // toggle watch back on if it was hidden
            if !any_shown && *o.state.name == *WATCH_NAME {
                o.state.spawn_scale = WATCH_SCALE * app.session.config.watch_scale;
            }
        })
    }
}

#[derive(Clone)]
pub enum OverlaySelector {
    Id(usize),
    Name(Arc<str>),
}

struct AppTask {
    pub not_before: Instant,
    pub task: TaskType,
}

impl PartialEq<AppTask> for AppTask {
    fn eq(&self, other: &Self) -> bool {
        self.not_before == other.not_before
    }
}
impl PartialOrd<AppTask> for AppTask {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.not_before.cmp(&other.not_before).reverse())
    }
}
impl Eq for AppTask {}
impl Ord for AppTask {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.not_before.cmp(&other.not_before).reverse()
    }
}

pub enum TaskType {
    Global(Box<dyn FnOnce(&mut AppState) + Send>),
    Overlay(
        OverlaySelector,
        Box<dyn FnOnce(&mut AppState, &mut OverlayState) + Send>,
    ),
    CreateOverlay(
        OverlaySelector,
        Box<dyn FnOnce(&mut AppState) -> Option<(OverlayState, Box<dyn OverlayBackend>)> + Send>,
    ),
    DropOverlay(OverlaySelector),
    Toast(Toast),
}

pub struct TaskContainer {
    tasks: BinaryHeap<AppTask>,
}

impl TaskContainer {
    pub fn new() -> Self {
        Self {
            tasks: BinaryHeap::new(),
        }
    }

    pub fn enqueue(&mut self, task: TaskType) {
        self.tasks.push(AppTask {
            not_before: Instant::now(),
            task,
        });
    }

    pub fn enqueue_at(&mut self, task: TaskType, not_before: Instant) {
        self.tasks.push(AppTask { not_before, task });
    }

    pub fn retrieve_due(&mut self, dest_buf: &mut VecDeque<TaskType>) {
        let now = Instant::now();

        while let Some(task) = self.tasks.peek() {
            if task.not_before > now {
                break;
            }

            // Safe unwrap because we peeked.
            dest_buf.push_back(self.tasks.pop().unwrap().task);
        }
    }
}

pub fn raycast(
    source: &Affine3A,
    source_fwd: Vec3A,
    plane: &Affine3A,
    plane_norm: Vec3A,
) -> Option<(Vec3A, f32)> {
    let plane_normal = plane.transform_vector3a(plane_norm);
    let ray_dir = source.transform_vector3a(source_fwd);

    let d = plane.translation.dot(-plane_normal);
    let dist = -(d + source.translation.dot(plane_normal)) / ray_dir.dot(plane_normal);

    if dist < 0.0 {
        // plane is behind the caster
        return None;
    }

    let hit_pos = source.translation + ray_dir * dist;
    Some((hit_pos, dist))
}
