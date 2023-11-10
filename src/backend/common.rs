use std::{
    collections::{BinaryHeap, VecDeque},
    sync::Arc,
    time::Instant,
};

use idmap::IdMap;

use crate::{
    overlays::{
        keyboard::create_keyboard,
        screen::{get_screens_wayland, get_screens_x11},
        watch::create_watch,
    },
    state::AppState,
};

use super::overlay::{OverlayData, OverlayState};

pub struct OverlayContainer<T>
where
    T: Default,
{
    overlays: IdMap<usize, OverlayData<T>>,
}

impl<T> OverlayContainer<T>
where
    T: Default,
{
    pub fn new(app: &mut AppState) -> Self {
        let mut overlays = IdMap::new();

        let screens = if std::env::var("WAYLAND_DISPLAY").is_ok() {
            get_screens_wayland(&app.session)
        } else {
            get_screens_x11()
        };

        let watch = create_watch::<T>(&app, &screens);
        overlays.insert(watch.state.id, watch);

        let keyboard = create_keyboard(&app);
        overlays.insert(keyboard.state.id, keyboard);

        let mut first = true;
        for mut screen in screens {
            if first {
                screen.state.want_visible = true;
                first = false;
            }
            overlays.insert(screen.state.id, screen);
        }

        Self { overlays }
    }

    pub fn mut_by_selector(&mut self, selector: &OverlaySelector) -> Option<&mut OverlayData<T>> {
        match selector {
            OverlaySelector::Id(id) => self.mut_by_id(*id),
            OverlaySelector::Name(name) => self.mut_by_name(name),
        }
    }

    pub fn get_by_id<'a>(&'a mut self, id: usize) -> Option<&'a OverlayData<T>> {
        self.overlays.get(&id)
    }

    pub fn mut_by_id<'a>(&'a mut self, id: usize) -> Option<&'a mut OverlayData<T>> {
        self.overlays.get_mut(&id)
    }

    pub fn get_by_name<'a>(&'a mut self, name: &str) -> Option<&'a OverlayData<T>> {
        self.overlays.values().find(|o| *o.state.name == *name)
    }

    pub fn mut_by_name<'a>(&'a mut self, name: &str) -> Option<&'a mut OverlayData<T>> {
        self.overlays.values_mut().find(|o| *o.state.name == *name)
    }

    pub fn iter<'a>(&'a self) -> impl Iterator<Item = &'a OverlayData<T>> {
        self.overlays.values()
    }

    pub fn iter_mut<'a>(&'a mut self) -> impl Iterator<Item = &'a mut OverlayData<T>> {
        self.overlays.values_mut()
    }
}

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

            dest_buf.push_back(self.tasks.pop().unwrap().task);
        }
    }
}
