use idmap_derive::IntegerId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, IntegerId, Serialize, Deserialize)]
pub enum ToastTopic {
	System,
	Error,
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

#[derive(Clone, Copy)]
pub enum BackendAttrib {
	Stereo,
	MouseTransform,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BackendAttribValue {
	Stereo(StereoMode),
	MouseTransform(MouseTransform),
}

#[derive(Default, Debug, Clone, Copy, Serialize, Deserialize)]
pub enum StereoMode {
	#[default]
	None,
	LeftRight,
	RightLeft,
	TopBottom,
	BottomTop,
}

#[derive(Default, Debug, Clone, Copy, Serialize, Deserialize)]
pub enum MouseTransform {
	#[default]
	Default,
	Normal,
	Rotated90,
	Rotated180,
	Rotated270,
	Flipped,
	Flipped90,
	Flipped180,
	Flipped270,
}
