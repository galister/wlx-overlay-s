use std::{sync::LazyLock, time::Instant};

static TIME_START: LazyLock<Instant> = LazyLock::new(Instant::now);

pub fn get_micros() -> u64 {
	TIME_START.elapsed().as_micros() as u64
}

pub struct Profiler {
	interval_us: u64,
	last_measure_us: u64,
	frametime_sum_us: u64,
	measure_frames: u64,
	time_start_us: u64,
}

impl Profiler {
	pub fn new(interval_ms: u64) -> Self {
		Self {
			frametime_sum_us: 0,
			interval_us: interval_ms * 1000,
			last_measure_us: 0,
			measure_frames: 0,
			time_start_us: 0,
		}
	}

	pub fn start(&mut self) {
		self.time_start_us = get_micros();
	}

	pub fn end(&mut self) {
		let cur_micros = get_micros();

		let frametime = cur_micros - self.time_start_us;
		self.measure_frames += 1;
		self.frametime_sum_us += frametime;

		if self.last_measure_us + self.interval_us < cur_micros {
			log::debug!(
				"avg frametime: {:.3}ms",
				(self.frametime_sum_us / self.measure_frames) as f32 / 1000.0
			);
			self.last_measure_us = cur_micros;
			self.frametime_sum_us = 0;
			self.measure_frames = 0;
		}
	}
}
