use std::fmt::Debug;

pub trait LogErr {
	fn log_err(self) -> Self;
	fn log_err_with(self, additional: &str) -> Self;
	fn log_warn(self) -> Self;
	fn log_warn_with(self, additional: &str) -> Self;
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

	fn log_warn_with(self, additional: &str) -> Result<T, E> {
		match self {
			Ok(ok) => Ok(ok),
			Err(error) => {
				log::warn!("{additional}: {error:?}");
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

	fn log_err_with(self, additional: &str) -> Result<T, E> {
		match self {
			Ok(ok) => Ok(ok),
			Err(error) => {
				log::error!("{additional}: {error:?}");
				Err(error)
			}
		}
	}
}
