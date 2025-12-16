use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket},
    time::Instant,
};

use anyhow::bail;
use rosc::{OscMessage, OscPacket, OscType};

use crate::{
    backend::input::TrackedDevice,
    overlays::{keyboard::KEYBOARD_NAME, watch::WATCH_NAME},
    windowing::manager::OverlayWindowManager,
};

use crate::backend::input::TrackedDeviceRole;

pub struct OscSender {
    last_sent_overlay: Instant,
    last_sent_device: Instant,
    upstream: UdpSocket,
}

impl OscSender {
    pub fn new(send_port: u16) -> anyhow::Result<Self> {
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);

        let Ok(upstream) = UdpSocket::bind("0.0.0.0:0") else {
            bail!("Failed to bind UDP socket - OSC will not function.");
        };

        let Ok(()) = upstream.connect(SocketAddr::new(ip, send_port)) else {
            bail!("Failed to connect UDP socket - OSC will not function.");
        };

        Ok(Self {
            upstream,
            last_sent_overlay: Instant::now(),
            last_sent_device: Instant::now(),
        })
    }

    pub fn send_message(&self, addr: String, args: Vec<OscType>) -> anyhow::Result<()> {
        let packet = OscPacket::Message(OscMessage { addr, args });
        let Ok(bytes) = rosc::encoder::encode(&packet) else {
            bail!("Could not encode OSC packet.");
        };

        let Ok(_) = self.upstream.send(&bytes) else {
            bail!("Could not send OSC packet.");
        };

        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    pub fn send_params<D>(
        &mut self,
        overlay_manager: &OverlayWindowManager<D>,
        devices: &Vec<TrackedDevice>,
    ) -> anyhow::Result<()>
    where
        D: Default,
    {
        // send overlay parameters every 0.1 seconds
        if self.last_sent_overlay.elapsed().as_millis() >= 100 {
            self.last_sent_overlay = Instant::now();

            let edit_mode = overlay_manager.get_edit_mode();
            let current_set = overlay_manager.get_current_set().unwrap_or(0) as i32;
            let total_sets = overlay_manager.get_total_sets() as i32;

            // check state of each active overlay and count them
            let mut num_overlays = 0;
            let mut has_keyboard = false;
            let mut has_wrist = false;
            for o in overlay_manager.values() {
                let Some(state) = o.config.active_state.as_ref() else {
                    continue;
                };

                // skip overlays that are fully transparent; e.g. the watch when not looking at it
                if state.alpha <= 0f32 {
                    continue;
                }

                match o.config.name.as_ref() {
                    WATCH_NAME => has_wrist = true,
                    KEYBOARD_NAME => has_keyboard = true,
                    _ => {
                        if state.interactable {
                            num_overlays += 1;
                        }
                    }
                }
            }

            // overlays
            self.send_message(
                "/avatar/parameters/isOverlayOpen".into(),
                vec![OscType::Bool(num_overlays > 0)],
            )?;
            self.send_message(
                "/avatar/parameters/ToggleWindows".into(),
                vec![OscType::Bool(num_overlays > 0)],
            )?;
            self.send_message(
                "/avatar/parameters/openOverlayCount".into(),
                vec![OscType::Int(num_overlays)],
            )?;

            // working sets
            self.send_message(
                "/avatar/parameters/isEditModeActive".into(),
                vec![OscType::Bool(edit_mode)],
            )?;
            self.send_message(
                "/avatar/parameters/ToggleEditMode".into(),
                vec![OscType::Bool(edit_mode)],
            )?;
            self.send_message(
                "/avatar/parameters/currentWorkingSet".into(),
                vec![OscType::Int(current_set)],
            )?;
            self.send_message(
                "/avatar/parameters/CurrentProfile".into(),
                vec![OscType::Int(current_set)],
            )?;
            self.send_message(
                "/avatar/parameters/totalWorkingSets".into(),
                vec![OscType::Int(total_sets)],
            )?;

            // keyboard
            self.send_message(
                "/avatar/parameters/isKeyboardOpen".into(),
                vec![OscType::Bool(has_keyboard)],
            )?;
            self.send_message(
                "/avatar/parameters/ToggleKeyboard".into(),
                vec![OscType::Bool(has_keyboard)],
            )?;

            // watch
            self.send_message(
                "/avatar/parameters/isWristVisible".into(),
                vec![OscType::Bool(has_wrist)],
            )?;
        }

        // send device parameters every 10 seconds
        if self.last_sent_device.elapsed().as_millis() >= 10000 {
            self.last_sent_device = Instant::now();

            let mut tracker_count: i8 = 0;
            let mut controller_count: i8 = 0;
            let mut tracker_total_bat = 0.0;
            let mut controller_total_bat = 0.0;

            let mut lowest_battery = 1f32;

            for device in devices {
                let tracker_param;

                // soc is the battery level (set to device status.charge)
                let level = device.soc.unwrap_or(-1.0);
                let parameter = match device.role {
                    TrackedDeviceRole::None => continue,
                    TrackedDeviceRole::Hmd => "hmd",
                    TrackedDeviceRole::LeftHand => {
                        controller_count += 1;
                        controller_total_bat += level;
                        "leftController"
                    }
                    TrackedDeviceRole::RightHand => {
                        controller_count += 1;
                        controller_total_bat += level;
                        "rightController"
                    }
                    TrackedDeviceRole::Tracker => {
                        tracker_count += 1;
                        tracker_total_bat += level;
                        tracker_param = format!("tracker{tracker_count}");
                        tracker_param.as_str()
                    }
                };

                lowest_battery = lowest_battery.min(level);

                // send device battery parameters
                self.send_message(
                    format!("/avatar/parameters/{parameter}Battery"),
                    vec![OscType::Float(level)],
                )?;
                self.send_message(
                    format!("/avatar/parameters/{parameter}Charging"),
                    vec![OscType::Bool(device.charging)],
                )?;
            }

            // send controller- and tracker-specific battery parameters
            self.send_message(
                String::from("/avatar/parameters/averageControllerBattery"),
                vec![OscType::Float(
                    controller_total_bat / f32::from(controller_count),
                )],
            )?;
            self.send_message(
                String::from("/avatar/parameters/averageTrackerBattery"),
                vec![OscType::Float(tracker_total_bat / f32::from(tracker_count))],
            )?;
            self.send_message(
                String::from("/avatar/parameters/LowestBattery"),
                vec![OscType::Float(lowest_battery)],
            )?;
            self.send_message(
                String::from("/avatar/parameters/lowestBattery"),
                vec![OscType::Float(lowest_battery)],
            )?;
        }

        Ok(())
    }
}

pub fn parse_osc_value(s: &str) -> anyhow::Result<OscType> {
    let lower = s.to_lowercase();

    match lower.as_str() {
        "true" => Ok(OscType::Bool(true)),
        "false" => Ok(OscType::Bool(false)),
        "inf" => Ok(OscType::Inf),
        "nil" => Ok(OscType::Nil),
        _ => {
            if lower.len() > 3 {
                let (num, suffix) = lower.split_at(lower.len() - 3);

                match suffix {
                    "f32" => return Ok(OscType::Float(num.parse::<f32>()?)),
                    "f64" => return Ok(OscType::Double(num.parse::<f64>()?)),
                    "i32" => return Ok(OscType::Int(num.parse::<i32>()?)),
                    "i64" => return Ok(OscType::Long(num.parse::<i64>()?)),
                    _ => {}
                }
            }

            anyhow::bail!("Unknown OSC type literal: {s}")
        }
    }
}
