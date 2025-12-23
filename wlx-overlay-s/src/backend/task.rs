use std::{
    cmp,
    collections::{BinaryHeap, VecDeque},
    sync::atomic::{self, AtomicUsize},
    time::Instant,
};

use serde::Deserialize;

use crate::{
    state::AppState,
    windowing::{OverlaySelector, window::OverlayWindowConfig},
};

#[cfg(feature = "wayvr")]
use crate::backend::wayvr::WayVRAction;

static TASK_AUTO_INCREMENT: AtomicUsize = AtomicUsize::new(0);

struct AppTask {
    pub not_before: Instant,
    pub id: usize,
    pub task: TaskType,
}

impl PartialEq<Self> for AppTask {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == cmp::Ordering::Equal
    }
}
impl PartialOrd<Self> for AppTask {
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

#[cfg(feature = "openvr")]
pub enum OpenVrTask {
    ColorGain(ColorChannel, f32),
}

pub enum PlayspaceTask {
    Recenter,
    Reset,
    FixFloor,
}

#[derive(Debug, Clone)]
pub enum ModifyPanelCommand {
    SetText(String),
    SetColor(String),
    SetImage(String),
    SetVisible(bool),
    SetStickyState(bool),
}

#[derive(Debug, Clone)]
pub struct ModifyPanelTask {
    pub overlay: String,
    pub element: String,
    pub command: ModifyPanelCommand,
}

pub type ModifyOverlayTask = dyn FnOnce(&mut AppState, &mut OverlayWindowConfig) + Send;
pub type CreateOverlayTask = dyn FnOnce(&mut AppState) -> Option<OverlayWindowConfig> + Send;
pub enum OverlayTask {
    AddSet,
    ToggleSet(usize),
    SoftToggleOverlay(OverlaySelector),
    DeleteActiveSet,
    ToggleEditMode,
    ShowHide,
    CleanupMirrors,
    Modify(OverlaySelector, Box<ModifyOverlayTask>),
    Create(OverlaySelector, Box<CreateOverlayTask>),
    ModifyPanel(ModifyPanelTask),
    Drop(OverlaySelector),
}

pub enum TaskType {
    Overlay(OverlayTask),
    Playspace(PlayspaceTask),
    #[cfg(feature = "openvr")]
    OpenVR(OpenVrTask),
    #[cfg(feature = "wayvr")]
    WayVR(WayVRAction),
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
    pub const fn new() -> Self {
        Self {
            tasks: BinaryHeap::new(),
        }
    }

    pub fn transfer_from(&mut self, other: &mut Self) {
        self.tasks.append(&mut other.tasks);
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
