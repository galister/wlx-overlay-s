use std::{sync::LazyLock, time::Instant};
static TIME_START: LazyLock<Instant> = LazyLock::new(Instant::now);

pub fn get_micros() -> u64 {
	TIME_START.elapsed().as_micros() as u64
}

#[derive(Default)]
pub struct Timestep {
	current_time_us: u64,
	accumulator: f32,
	time_micros: u64,
	ticks: u32,
	speed: f32,
	pub alpha: f32,
	delta: f32,
	loopnum: u8,
}

impl Timestep {
	pub fn new() -> Timestep {
		let mut timestep = Timestep {
			speed: 1.0,
			..Default::default()
		};

		timestep.reset();
		timestep
	}

	fn calculate_alpha(&mut self) {
		self.alpha = (self.accumulator / self.delta).clamp(0.0, 1.0);
	}

	pub fn set_tps(&mut self, tps: f32) {
		self.delta = 1000.0 / tps;
	}

	pub fn reset(&mut self) {
		self.current_time_us = get_micros();
		self.accumulator = 0.0;
	}

	pub fn on_tick(&mut self) -> bool {
		let newtime = get_micros();
		let frametime = newtime - self.current_time_us;
		self.time_micros += frametime;
		self.current_time_us = newtime;
		self.accumulator += frametime as f32 * self.speed / 1000.0;
		self.calculate_alpha();

		if self.accumulator >= self.delta {
			self.accumulator -= self.delta;
			self.loopnum += 1;
			self.ticks += 1;

			if self.loopnum > 5 {
				// cannot keep up!
				self.loopnum = 0;
				self.accumulator = 0.0;
				return false;
			}

			true
		} else {
			self.loopnum = 0;
			false
		}
	}
}
