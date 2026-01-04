use std::fmt::Debug;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Default)]
#[repr(u8)]
pub enum LeftRight {
	#[default]
	Left,
	Right,
}

pub trait LogErr {
	fn log_err(self) -> Self;
	fn log_warn(self) -> Self;
}

impl<T, E> LogErr for Result<T, E>
where
	E: Debug + Send + Sync + 'static,
{
	fn log_warn(self) -> Result<T, E> {
		match self {
			Ok(ok) => Ok(ok),
			Err(error) => {
				log::warn!("{error:?}");
				Err(error)
			}
		}
	}

	fn log_err(self) -> Result<T, E> {
		match self {
			Ok(ok) => Ok(ok),
			Err(error) => {
				log::error!("{error:?}");
				Err(error)
			}
		}
	}
}
