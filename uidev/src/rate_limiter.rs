use crate::timestep::get_micros;

#[derive(Default)]
pub struct RateLimiter {
	rate: u16,
	start_us: u64,
	end_us: u64,
}

impl RateLimiter {
	pub fn new() -> Self {
		Self {
			rate: 0,
			end_us: 0,
			start_us: 0,
		}
	}

	pub fn start(&mut self, rate: u16) {
		self.rate = rate;
		self.start_us = get_micros();
	}

	pub fn end(&mut self) {
		if self.rate == 0 {
			return;
		}

		self.end_us = get_micros();
		let microseconds = self.end_us - self.start_us;
		let frametime_microseconds = ((1000.0 / self.rate as f32) * 1000.0) as u64;
		let delay = frametime_microseconds as i64 - microseconds as i64;
		if delay > 0 {
			std::thread::sleep(std::time::Duration::from_micros(delay as u64));
		}
	}
}
