use std::{
    cmp,
    collections::{BinaryHeap, VecDeque},
    sync::atomic::{self, AtomicUsize},
    time::Instant,
};

#[cfg(feature = "wayvr")]
use std::sync::Arc;

use serde::Deserialize;

use crate::state::AppState;

use super::{
    common::OverlaySelector,
    overlay::{OverlayBackend, OverlayState},
};

static TASK_AUTO_INCREMENT: AtomicUsize = AtomicUsize::new(0);

struct AppTask {
    pub not_before: Instant,
    pub id: usize,
    pub task: TaskType,
}

impl PartialEq<AppTask> for AppTask {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == cmp::Ordering::Equal
    }
}
impl PartialOrd<AppTask> for AppTask {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Eq for AppTask {}
impl Ord for AppTask {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.not_before
            .cmp(&other.not_before)
            .then(self.id.cmp(&other.id))
            .reverse()
    }
}

pub enum SystemTask {
    ColorGain(ColorChannel, f32),
    ResetPlayspace,
    FixFloor,
    ShowHide,
}

#[cfg(feature = "wayvr")]
pub struct WayVRTask {
    pub catalog_name: Arc<str>,
    pub app_name: Arc<str>,
}

pub type OverlayTask = dyn FnOnce(&mut AppState, &mut OverlayState) + Send;
pub type CreateOverlayTask =
    dyn FnOnce(&mut AppState) -> Option<(OverlayState, Box<dyn OverlayBackend>)> + Send;

pub enum TaskType {
    Global(Box<dyn FnOnce(&mut AppState) + Send>),
    Overlay(OverlaySelector, Box<OverlayTask>),
    CreateOverlay(OverlaySelector, Box<CreateOverlayTask>),
    DropOverlay(OverlaySelector),
    System(SystemTask),
    #[cfg(feature = "wayvr")]
    WayVR(WayVRTask),
}

#[derive(Deserialize, Clone, Copy)]
pub enum ColorChannel {
    R,
    G,
    B,
    All,
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
            id: TASK_AUTO_INCREMENT.fetch_add(1, atomic::Ordering::Relaxed),
            task,
        });
    }

    /// Enqueue a task to be executed at a specific time.
    /// If the time is in the past, the task will be executed immediately.
    /// Multiple tasks enqueued for the same instant will be executed in order of submission.
    pub fn enqueue_at(&mut self, task: TaskType, not_before: Instant) {
        self.tasks.push(AppTask {
            not_before,
            id: TASK_AUTO_INCREMENT.fetch_add(1, atomic::Ordering::Relaxed),
            task,
        });
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
