use glam::Affine3A;
use serde::{Deserialize, Serialize};

use crate::common::LeftRight;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub enum Positioning {
	/// Stays in place, recenters relative to HMD
	#[default]
	Floating,
	/// Stays in place, recenters relative to anchor. Follows anchor during anchor grab.
	Anchored,
	/// Stays in place, no recentering
	Static,
	/// Following HMD
	FollowHead { lerp: f32 },
	/// Following hand
	FollowHand { hand: LeftRight, lerp: f32 },
}

impl Positioning {
	pub const fn moves_with_space(self) -> bool {
		matches!(self, Self::Floating | Self::Anchored | Self::Static)
	}
}

// Contains the window state for a given set
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OverlayWindowState {
	pub transform: Affine3A,
	pub alpha: f32,
	pub grabbable: bool,
	pub interactable: bool,
	pub positioning: Positioning,
	pub curvature: Option<f32>,
	pub additive: bool,
	#[serde(skip_serializing, skip_deserializing)]
	pub saved_transform: Option<Affine3A>,
}

impl Default for OverlayWindowState {
	fn default() -> Self {
		Self {
			grabbable: false,
			interactable: false,
			alpha: 1.0,
			positioning: Positioning::Floating,
			curvature: None,
			transform: Affine3A::IDENTITY,
			additive: false,
			saved_transform: None,
		}
	}
}
