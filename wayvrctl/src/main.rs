use std::{collections::HashMap, process::{self, ExitCode}, time::Duration};

use anyhow::Context;
use clap::Parser;
use env_logger::Env;
use wayvr_ipc::{client::WayVRClient, ipc, packet_client, };

use crate::helper::{wlx_haptics, wlx_input_state, wlx_panel_modify, wvr_display_create, wvr_display_get, wvr_display_list, wvr_display_remove, wvr_display_set_visible, wvr_display_window_list, wvr_process_get, wvr_process_launch, wvr_process_list, wvr_process_terminate, wvr_window_set_visible, WayVRClientState};

mod helper;


#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    env_logger::init_from_env(Env::default().default_filter_or("info"));
    let args = Args::parse();

    let mut state = WayVRClientState {
        wayvr_client : WayVRClient::new(&format!("wayvrctl-{}", process::id())).await.inspect_err(|e| {
            log::error!("Failed to initialize WayVR connection: {e:?}");
            process::exit(1);
        }).unwrap(),
        serial_generator: ipc::SerialGenerator::new(),
        pretty_print: args.pretty,
    };

    let maybe_err = if let Subcommands::Batch {fail_fast} = args.command {
        run_batch(&mut state, fail_fast).await
    } else {
        run_once(&mut state, args).await
     };

    if let Err(e) = maybe_err{
        log::error!("{e:?}");
        return ExitCode::FAILURE;
    } else {
        std::thread::sleep(Duration::from_millis(20));
    }

    ExitCode::SUCCESS
}

async fn run_batch(state: &mut WayVRClientState, fail_fast: bool) -> anyhow::Result<()> {
    let stdin = std::io::stdin();

    for (line_no, line) in stdin.lines().enumerate() {
        let line = line.context("error reading stdin")?;

        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }

        if let Err(e) = parse_run_line(state, &line).await.with_context(|| format!("error on line {}", line_no + 1)) {
            if fail_fast {
                return Err(e)
            } else {
                log::error!("{e:?}");
            }
        }

    }
    Ok(())
}

async fn parse_run_line(state: &mut WayVRClientState, line: &str) -> anyhow::Result<()> {
            let mut argv = shell_words::split(&line)
            .with_context(|| format!("parse error"))
            ?;

        // clap expects argv[0] to be the binary name
        argv.insert(0, env!("CARGO_PKG_NAME").to_string());

        let args = Args::try_parse_from(argv).with_context(|| format!("invalid arguments"))?;
        run_once(state, args).await?;

        Ok(())
}

async fn run_once(state: &mut WayVRClientState, args: Args) -> anyhow::Result<()> {
    match args.command {
        Subcommands::Batch { .. } => {
            log::warn!("Ignoring recursive batch command");
        }
        Subcommands::InputState => {
            wlx_input_state(state).await;
        }
        Subcommands::DisplayCreate { width, height, name, scale } => {
            wvr_display_create(state, width, height, name, scale, packet_client::AttachTo::None).await;
        }
        Subcommands::DisplayList => {
            wvr_display_list(state).await;
        }
        Subcommands::DisplayGet { handle } => {
            let handle = serde_json::from_str(&handle).context("Invalid handle")?;
            wvr_display_get(state, handle).await;
        }
        Subcommands::DisplayWindowList { handle } => {
            let handle = serde_json::from_str(&handle).context("Invalid handle")?;
            wvr_display_window_list(state, handle).await;
        }
        Subcommands::DisplayRemove { handle } => {
            let handle = serde_json::from_str(&handle).context("Invalid handle")?;
            wvr_display_remove(state, handle).await;
        }
        Subcommands::DisplaySetVisible { handle, visible_0_or_1 } => {
            let handle = serde_json::from_str(&handle).context("Invalid handle")?;
            wvr_display_set_visible(state, handle, visible_0_or_1 != 0).await;
        }
        Subcommands::WindowSetVisible { handle, visible_0_or_1 } => {
            let handle = serde_json::from_str(&handle).context("Invalid handle")?;
            wvr_window_set_visible(state, handle, visible_0_or_1 != 0).await;
        }
        Subcommands::ProcessGet { handle } => {
            let handle = serde_json::from_str(&handle).context("Invalid handle")?;
            wvr_process_get(state, handle).await;
        }
        Subcommands::ProcessList => {
            wvr_process_list(state).await;
        }
        Subcommands::ProcessTerminate { handle } => {
            let handle = serde_json::from_str(&handle).context("Invalid handle")?;
            wvr_process_terminate(state, handle).await;
        }
        Subcommands::ProcessLaunch { exec, name, env, target_display, args } => {
            let handle = serde_json::from_str(&target_display).context("Invalid target_display")?;
            wvr_process_launch(state, exec, name, env, handle, args, HashMap::new()).await;
        }
        Subcommands::Haptics { intensity, duration, frequency } => {
            wlx_haptics(state, intensity, duration, frequency).await;
        }
        Subcommands::PanelModify { overlay, element, command } => {
            let command = match command {
                SubcommandPanelModify::SetText { text } => packet_client::WlxModifyPanelCommand::SetText(text),
                SubcommandPanelModify::SetColor { hex_color } => packet_client::WlxModifyPanelCommand::SetColor(hex_color),
                SubcommandPanelModify::SetImage { absolute_path } => packet_client::WlxModifyPanelCommand::SetImage(absolute_path),
                SubcommandPanelModify::SetVisible { visible_0_or_1 } => packet_client::WlxModifyPanelCommand::SetVisible(visible_0_or_1 != 0),
                SubcommandPanelModify::SetStickyState { sticky_state_0_or_1 } => packet_client::WlxModifyPanelCommand::SetStickyState(sticky_state_0_or_1 != 0),
            };
            
            wlx_panel_modify(state, overlay, element, command).await;
        }
    }
    Ok(())
}


/// A command-line interface for WayVR IPC
#[derive(clap::Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The command to run
    #[command(subcommand)]
    command: Subcommands,

    /// Pretty-print JSON output
    #[arg(short, long)]
    pretty: bool,
}

#[derive(clap::Parser, Debug)]
enum Subcommands {
    /// Read commands from stdout, one per line.
    Batch {
    /// Stop on the first error
    #[arg(short, long)]
      fail_fast: bool,  
    },
    /// Get the positions of HMD & controllers
    InputState,
    /// Create a new WayVR display
    DisplayCreate{
    	width: u16,
    	height: u16,
    	name: String,
        #[arg(short, long)]
    	scale: Option<f32>,

    	//attach_to: packet_client::AttachTo,
    },
    /// List WayVR displays
    DisplayList,
    /// Retrieve information about a single WayVR display
    DisplayGet {
        /// A display handle JSON returned by DisplayList or DisplayCreate
    	handle: String,
    },
    /// List windows attached to a WayVR display
    DisplayWindowList {
        /// A display handle JSON returned by DisplayList or DisplayCreate
    	handle: String,
    },
    /// Delete a WayVR display
    DisplayRemove {
        /// A display handle JSON returned by DisplayList or DisplayCreate
    	handle: String,
    },
    /// Change the visibility of a WayVR display
    DisplaySetVisible {
        /// A display handle JSON returned by DisplayList or DisplayCreate
    	handle: String,
    	visible_0_or_1: u8,
    },

    // DisplaySetLayout skipped
 
    /// Change the visibility of a window on a WayVR display
    WindowSetVisible {
        /// A JSON window handle returned by DisplayWindowList
    	handle: String,
    	visible_0_or_1: u8,
    },
    /// Retrieve information about a WayVR-managed process
    ProcessGet {
    /// A JSON process handle returned by ProcessList or ProcessLaunch
	handle: String,

    },
    /// List all processes managed by WayVR
    ProcessList,
    /// Terminate a WayVR-managed process
    ProcessTerminate {
        /// A JSON process handle returned by ProcessList or ProcessLaunch
    	handle: String,
    },
    /// Launch a new process inside WayVR
    ProcessLaunch {
    	exec: String,
    	name: String,
    	env: Vec<String>,
        /// A display handle JSON returned by DisplayList or DisplayCreate
    	target_display: String,
    	args: String,
    },
    /// Trigger haptics on the user's controller
    Haptics {
        #[arg(short, long, default_value = "0.25")]
    	intensity: f32,
        #[arg(short, long , default_value = "0.1")]
    	duration: f32,
        #[arg(short, long, default_value = "0.1")]
    	frequency: f32,
    },
    /// Apply a modification to a panel element
    PanelModify {
        /// The name of the overlay (XML file name without extension)
    	overlay: String,
        /// The id of the element to modify, as set in the XML
    	element: String,
        /// Command to execute
        #[command(subcommand)]
    	command: SubcommandPanelModify,
    }

}

#[derive(clap::Parser, Debug)]
enum SubcommandPanelModify {
    /// Set the text of a <label> or <Button>
    SetText {
        /// Text that needs to be set
        text: String,
    },
    /// Set the color of a <rectangle> or <label> or monochrome <sprite>
    SetColor {
        /// Color in HTML hex format (#rrggbb or #rrggbbaa)
        hex_color: String,
    },
    /// Set the content of a <sprite> or <image>. Max size for <sprite> is 256x256.
    SetImage {
        /// Absolute path to a svg, gif, png, jpeg or webp image.
        absolute_path: String,
    },
    /// Set the visibility of a <div>, <rectangle>, <label>, <sprite> or <image>
    SetVisible {
        visible_0_or_1: u8,
    },
    /// Set the sticky state of a <Button>. Intended for buttons without `sticky="1"`.
    SetStickyState {
        sticky_state_0_or_1: u8,
    }
}
