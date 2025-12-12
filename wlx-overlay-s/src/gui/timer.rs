use std::time::{Duration, Instant};

pub struct GuiTimer {
    interval: Duration,
    next_tick: Instant,
    signal: usize,
}

impl GuiTimer {
    pub fn new(interval: Duration, signal: usize) -> Self {
        Self {
            interval,
            next_tick: Instant::now() + interval,
            signal,
        }
    }

    pub fn check_tick(&mut self) -> Option<usize> {
        if self.next_tick > Instant::now() {
            return None;
        }

        self.next_tick = Instant::now() + self.interval;
        Some(self.signal)
    }
}
