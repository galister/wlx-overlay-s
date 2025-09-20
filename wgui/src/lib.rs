#![warn(clippy::all, clippy::pedantic, clippy::nursery)]
#![allow(
	clippy::suboptimal_flops,
	clippy::cast_precision_loss,
	clippy::missing_errors_doc,
	clippy::default_trait_access,
	clippy::missing_panics_doc,
	clippy::cast_possible_wrap,
	clippy::cast_possible_truncation,
	clippy::cast_sign_loss,
	clippy::items_after_statements,
	clippy::future_not_send,
	clippy::must_use_candidate,
	clippy::implicit_hasher,
	clippy::option_if_let_else,
	clippy::significant_drop_tightening,
	clippy::float_cmp,
	clippy::needless_pass_by_ref_mut
)]

pub mod animation;
pub mod any;
pub mod assets;
pub mod components;
pub mod drawing;
pub mod event;
pub mod gfx;
pub mod globals;
pub mod i18n;
pub mod layout;
pub mod parser;
pub mod renderer_vk;
pub mod transform_stack;
pub mod widget;

// re-exported libs
pub use cosmic_text;
pub use taffy;
