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
	FollowHead {
		#[serde(default)]
		lerp: f32,
	},
	/// Following hand
	FollowHand {
		hand: LeftRight,
		#[serde(default)]
		lerp: f32,
		#[serde(default)]
		align_to_hmd: bool,
	},
}

impl Positioning {
	pub const fn moves_with_space(self) -> bool {
		matches!(self, Self::Floating | Self::Anchored | Self::Static)
	}
	pub const fn get_lerp(self) -> Option<f32> {
		match self {
			Self::FollowHead { lerp } => Some(lerp),
			Self::FollowHand { lerp, .. } => Some(lerp),
			Self::Floating | Self::Anchored | Self::Static => None,
		}
	}
	pub const fn with_lerp(mut self, value: f32) -> Self {
		match self {
			Self::FollowHead { ref mut lerp } => *lerp = value,
			Self::FollowHand { ref mut lerp, .. } => *lerp = value,
			Self::Floating | Self::Anchored | Self::Static => {}
		}
		self
	}
	pub const fn get_align(self) -> Option<bool> {
		match self {
			Self::FollowHand { align_to_hmd, .. } => Some(align_to_hmd),
			Self::FollowHead { .. } | Self::Floating | Self::Anchored | Self::Static => None,
		}
	}
	pub const fn with_align(mut self, value: bool) -> Self {
		match self {
			Self::FollowHand {
				ref mut align_to_hmd, ..
			} => *align_to_hmd = value,
			Self::FollowHead { .. } | Self::Floating | Self::Anchored | Self::Static => {}
		}
		self
	}
}

// Contains the window state for a given set
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OverlayWindowState {
	#[serde(skip_serializing, skip_deserializing)]
	pub transform: Affine3A,
	pub alpha: f32,
	pub grabbable: bool,
	pub interactable: bool,
	pub positioning: Positioning,
	pub curvature: Option<f32>,
	pub additive: bool,
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
