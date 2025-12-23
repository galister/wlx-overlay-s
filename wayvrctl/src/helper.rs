use std::{collections::HashMap};

use anyhow::Context;
use serde::Serialize;
use wayvr_ipc::{client::{WayVRClient, WayVRClientMutex}, ipc, packet_client, packet_server};

pub struct WayVRClientState {
	pub wayvr_client: WayVRClientMutex,
	pub serial_generator: ipc::SerialGenerator,
	pub pretty_print: bool,
}

fn handle_empty_result(result: anyhow::Result<()>) {
	if let Err(e) = result {
		log::error!("{e:?}");
	}
}

fn handle_result<T: Serialize>(pretty_print: bool, result: anyhow::Result<T>) {
	match result {
		Ok(t) => {
			
			let maybe_json = if pretty_print {
				serde_json::to_string_pretty(&t)
			} else {
				serde_json::to_string(&t)
			};

			match maybe_json {
				Ok(json_string) => println!("{}", json_string),
				Err(e) => log::error!("Failed to serialize JSON: {e:?}")
			}
		}
		Err(e) => log::error!("{e:?}")
	}
}

pub async fn wvr_display_create(
	state: &mut WayVRClientState,
	width: u16,
	height: u16,
	name: String,
	scale: Option<f32>,
	attach_to: packet_client::AttachTo,
) {
	handle_result(state.pretty_print,
		WayVRClient::fn_wvr_display_create(
			state.wayvr_client.clone(),
 			state.serial_generator.increment_get(),
			packet_client::WvrDisplayCreateParams {
				width,
				height,
				name,
				scale,
				attach_to,
			},
		)
		.await.context("failed to create display")
	);
}

pub async fn wvr_display_list(
	state: &mut WayVRClientState,
) {
	handle_result(state.pretty_print,
		WayVRClient::fn_wvr_display_list(
			state.wayvr_client.clone(),
 			state.serial_generator.increment_get(),
		)
		.await.context("failed to fetch displays")
);
}

pub async fn wvr_display_get(
	state: &mut WayVRClientState,
	handle: packet_server::WvrDisplayHandle,
) {
	handle_result(state.pretty_print,
		WayVRClient::fn_wvr_display_get(
			state.wayvr_client.clone(),
 			state.serial_generator.increment_get(),
			handle,
		)
		.await.context("failed to fetch display")
);
}

pub async fn wvr_display_window_list(
	state: &mut WayVRClientState,
	handle: packet_server::WvrDisplayHandle,
) {
	handle_result(state.pretty_print,
		WayVRClient::fn_wvr_display_window_list(
			state.wayvr_client.clone(),
 			state.serial_generator.increment_get(),
			handle,
		)
		.await.context("failed to list window displays")
	);
}

pub async fn wvr_display_remove(
	state: &mut WayVRClientState,
	handle: packet_server::WvrDisplayHandle,
) {
	handle_result(state.pretty_print,
		WayVRClient::fn_wvr_display_remove(
			state.wayvr_client.clone(),
 			state.serial_generator.increment_get(),
			handle,
		)
		.await.context("failed to remove display")
	);
}

pub async fn wvr_display_set_visible(
	state: &mut WayVRClientState,
	handle: packet_server::WvrDisplayHandle,
	visible: bool,
) {
	handle_empty_result(
		WayVRClient::fn_wvr_display_set_visible(state.wayvr_client.clone(), handle, visible).await.context("failed to set display visibility"),
	)
}

pub async fn wvr_window_set_visible(
	state: &mut WayVRClientState,
	handle: packet_server::WvrWindowHandle,
	visible: bool,
) {
	handle_empty_result(
		WayVRClient::fn_wvr_window_set_visible(state.wayvr_client.clone(), handle, visible).await.context("failed to set window visibility"),
	)
}

pub async fn wvr_process_get(
	state: &mut WayVRClientState,
	handle: packet_server::WvrProcessHandle,
) {
	handle_result(state.pretty_print,
		WayVRClient::fn_wvr_process_get(
			state.wayvr_client.clone(),
 			state.serial_generator.increment_get(),
			handle,
		)
		.await.context("failed to get process"),
	);
}

pub async fn wvr_process_list(
	state: &mut WayVRClientState,
) {
	handle_result(state.pretty_print,
		WayVRClient::fn_wvr_process_list(
			state.wayvr_client.clone(),
 			state.serial_generator.increment_get(),
		)
		.await.context("failed to list processes"),
	)
}

pub async fn wvr_process_terminate(
	state: &mut WayVRClientState,
	handle: packet_server::WvrProcessHandle,
) {
	handle_empty_result(
		WayVRClient::fn_wvr_process_terminate(state.wayvr_client.clone(), handle).await.context("failed to terminate process"),
	)
}

pub async fn wvr_process_launch(
	state: &mut WayVRClientState,
	exec: String,
	name: String,
	env: Vec<String>,
	target_display: packet_server::WvrDisplayHandle,
	args: String,
	userdata: HashMap<String, String>,
) {
	handle_result(state.pretty_print,
		WayVRClient::fn_wvr_process_launch(
			state.wayvr_client.clone(),
			state.serial_generator.increment_get(),
			packet_client::WvrProcessLaunchParams {
				env,
				exec,
				name,
				target_display,
				args,
				userdata,
			},
		)
		.await.context("failed to launch process"),
	)
}

pub async fn wlx_haptics(
	state: &mut WayVRClientState,
	intensity: f32,
	duration: f32,
	frequency: f32,
) {
	handle_empty_result(
		WayVRClient::fn_wlx_haptics(
			state.wayvr_client.clone(),
			packet_client::WlxHapticsParams {
				intensity,
				duration,
				frequency,
			},
		)
		.await.context("failed to trigger haptics"),
	)
}

pub async fn wlx_panel_modify(
	state: &mut WayVRClientState,
	overlay: String,
	element: String,
	command: packet_client::WlxModifyPanelCommand,
) {
	handle_empty_result(
		WayVRClient::fn_wlx_modify_panel(
			state.wayvr_client.clone(),
			packet_client::WlxModifyPanelParams {
				overlay,
				element,
				command,
			},
		)
		.await.context("failed to modify panel"),
	)
}

pub async fn wlx_input_state(
	state: &mut WayVRClientState,
) {
	handle_result(state.pretty_print,
		WayVRClient::fn_wlx_input_state(
			state.wayvr_client.clone(),
 			state.serial_generator.increment_get(),
		)
		.await.context("failed to get input state"),
	)
}

