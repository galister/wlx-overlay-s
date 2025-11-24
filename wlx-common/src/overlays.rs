use idmap_derive::IntegerId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, IntegerId, Serialize, Deserialize)]
pub enum ToastTopic {
	System,
	DesktopNotification,
	XSNotification,
	IpdChange,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ToastDisplayMethod {
	Hide,
	Center,
	Watch,
}
