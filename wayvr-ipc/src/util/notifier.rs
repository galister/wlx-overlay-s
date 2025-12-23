use std::sync::Arc;

use tokio::sync::Notify;

// Copyable wrapped Notify struct for easier usage
#[derive(Default, Clone)]
pub struct Notifier {
	notifier: Arc<Notify>,
}

impl Notifier {
	pub fn new() -> Self {
		Self {
			notifier: Arc::new(Notify::new()),
		}
	}

	pub fn notify(&self) {
		self.notifier.notify_waiters();
	}

	pub async fn wait(&self) {
		self.notifier.notified().await;
	}
}
