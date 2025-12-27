use wayvr_ipc::packet_server;

#[cfg(feature = "wayvr")]
use crate::backend::wayvr::{self, WvrServerState};

use crate::{
    backend::{
        self,
        task::{InputTask, OverlayTask, TaskType},
    },
    ipc::signal::WayVRSignal,
    state::AppState,
    windowing::{OverlaySelector, manager::OverlayWindowManager},
};

#[cfg(feature = "wayvr")]
fn process_tick_tasks(
    tick_tasks: Vec<backend::wayvr::TickTask>,
    server_state: &mut WvrServerState,
) -> anyhow::Result<()> {
    for tick_task in tick_tasks {
        match tick_task {
            backend::wayvr::TickTask::NewExternalProcess(request) => {
                log::info!("Registering external process with PID {}", request.pid);
                server_state.add_external_process(request.pid);
                server_state.manager.add_client(wayvr::client::WayVRClient {
                    client: request.client,
                    pid: request.pid,
                });
            }
        }
    }

    Ok(())
}

pub fn tick_events<O>(
    app: &mut AppState,
    _overlays: &mut OverlayWindowManager<O>,
) -> anyhow::Result<()>
where
    O: Default,
{
    while let Some(signal) = app.wayvr_signals.read() {
        match signal {
            #[cfg(feature = "wayvr")]
            WayVRSignal::BroadcastStateChanged(packet) => {
                app.ipc_server
                    .broadcast(packet_server::PacketServer::WvrStateChanged(packet));
            }
            WayVRSignal::DeviceHaptics(device, haptics) => {
                app.tasks
                    .enqueue(TaskType::Input(InputTask::Haptics { device, haptics }));
            }
            WayVRSignal::DropOverlay(overlay_id) => {
                app.tasks
                    .enqueue(TaskType::Overlay(OverlayTask::Drop(OverlaySelector::Id(
                        overlay_id,
                    ))));
            }
            WayVRSignal::CustomTask(custom_task) => {
                app.tasks
                    .enqueue(TaskType::Overlay(OverlayTask::ModifyPanel(custom_task)));
            }
        }
    }

    #[cfg(feature = "wayvr")]
    {
        let tick_tasks = WvrServerState::tick_events(app)?;
        if let Some(wayvr_server) = app.wvr_server.as_mut() {
            process_tick_tasks(tick_tasks, wayvr_server)?;
        }
    }

    #[cfg(not(feature = "wayvr"))]
    {
        use super::ipc_server::TickParams;
        app.ipc_server.tick(&mut TickParams {
            input_state: &app.input_state,
            signals: &app.wayvr_signals,
        });
    }

    Ok(())
}
