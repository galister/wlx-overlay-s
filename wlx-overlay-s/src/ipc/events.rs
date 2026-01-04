use std::{cell::RefCell, rc::Rc};

use wayvr_ipc::packet_server;

#[cfg(feature = "wayvr")]
use crate::{
    backend::wayvr, config_wayvr, overlays::wayvr::OverlayToCreate, overlays::wayvr::WayVRData,
};

use crate::{
    backend::{
        self,
        task::{InputTask, OverlayTask, TaskType},
    },
    ipc::signal::WayVRSignal,
    overlays::{self},
    state::AppState,
    windowing::{OverlaySelector, manager::OverlayWindowManager},
};

#[cfg(feature = "wayvr")]
fn process_tick_tasks(
    app: &mut AppState,
    tick_tasks: Vec<backend::wayvr::TickTask>,
    r_wayvr: &Rc<RefCell<WayVRData>>,
) -> anyhow::Result<()> {
    for tick_task in tick_tasks {
        match tick_task {
            backend::wayvr::TickTask::NewExternalProcess(request) => {
                let config = &app.session.wayvr_config;

                let disp_name = request.env.display_name.map_or_else(
                    || {
                        config
                            .get_default_display()
                            .map(|(display_name, _)| display_name)
                    },
                    |display_name| {
                        config
                            .get_display(display_name.as_str())
                            .map(|_| display_name)
                    },
                );

                if let Some(disp_name) = disp_name {
                    let mut wayvr = r_wayvr.borrow_mut();

                    log::info!("Registering external process with PID {}", request.pid);

                    let disp_handle = overlays::wayvr::get_or_create_display_by_name(
                        app, &mut wayvr, &disp_name,
                    )?;

                    wayvr
                        .data
                        .state
                        .add_external_process(disp_handle, request.pid);

                    wayvr
                        .data
                        .state
                        .manager
                        .add_client(wayvr::client::WayVRClient {
                            client: request.client,
                            display_handle: disp_handle,
                            pid: request.pid,
                        });
                }
            }
            wayvr::TickTask::NewDisplay(cpar, disp_handle) => {
                log::info!("Creating new display with name \"{}\"", cpar.name);

                let mut wayvr = r_wayvr.borrow_mut();

                let unique_name = wayvr.get_unique_display_name(cpar.name);

                let disp_handle = match disp_handle {
                    Some(d) => d,
                    None => wayvr.data.state.create_display(
                        cpar.width,
                        cpar.height,
                        &unique_name,
                        false,
                    )?,
                };

                wayvr.overlays_to_create.push(OverlayToCreate {
                    disp_handle,
                    conf_display: config_wayvr::WayVRDisplay {
                        attach_to: Some(config_wayvr::AttachTo::from_packet(&cpar.attach_to)),
                        width: cpar.width,
                        height: cpar.height,
                        pos: None,
                        primary: None,
                        rotation: None,
                        scale: cpar.scale,
                    },
                });
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_lines)]
pub fn tick_events<O>(
    app: &mut AppState,
    overlays: &mut OverlayWindowManager<O>,
) -> anyhow::Result<()>
where
    O: Default,
{
    #[cfg(feature = "wayvr")]
    let wayland_server = app.wayland_server.clone();

    while let Some(signal) = app.wayvr_signals.read() {
        match signal {
            #[cfg(feature = "wayvr")]
            WayVRSignal::DisplayVisibility(display_handle, visible) => {
                if let Some(mut wayland_server) = wayland_server.as_ref().map(|r| r.borrow_mut())
                    && let Some(overlay_id) = wayland_server.display_handle_map.get(&display_handle)
                {
                    let overlay_id = *overlay_id;
                    wayland_server
                        .data
                        .state
                        .set_display_visible(display_handle, visible);
                    app.tasks.enqueue(TaskType::Overlay(OverlayTask::Modify(
                        OverlaySelector::Id(overlay_id),
                        Box::new(move |app, o| {
                            if visible == o.is_active() {
                                return;
                            }
                            if visible {
                                o.activate(app);
                            } else {
                                o.deactivate();
                            }
                        }),
                    )));
                }
            }
            #[cfg(feature = "wayvr")]
            WayVRSignal::DisplayWindowLayout(display_handle, layout) => {
                if let Some(mut wayland_server) = wayland_server.as_ref().map(|r| r.borrow_mut()) {
                    wayland_server
                        .data
                        .state
                        .set_display_layout(display_handle, layout);
                }
            }
            #[cfg(feature = "wayvr")]
            WayVRSignal::BroadcastStateChanged(packet) => {
                app.ipc_server
                    .broadcast(packet_server::PacketServer::WvrStateChanged(packet));
            }
            #[cfg(feature = "wayvr")]
            WayVRSignal::Haptics(haptics) => {
                if let Some(mut wayland_server) = wayland_server.as_ref().map(|r| r.borrow_mut()) {
                    wayland_server.pending_haptics = Some(haptics);
                }
            }
            WayVRSignal::DeviceHaptics(device, haptics) => {
                app.tasks
                    .enqueue(TaskType::Input(InputTask::Haptics { device, haptics }));
            }
            WayVRSignal::ShowHide => {
                app.tasks
                    .enqueue(TaskType::Overlay(OverlayTask::ShowHide));
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
        if let Some(wayland_server) = wayland_server {
            let tick_tasks = wayland_server.borrow_mut().data.tick_events(app)?;
            process_tick_tasks(app, tick_tasks, &wayland_server)?;

            overlays::wayvr::create_queued_displays(
                app,
                &mut wayland_server.borrow_mut(),
                overlays,
            )?;
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
