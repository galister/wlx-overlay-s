use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket},
    time::Instant,
};

use anyhow::bail;
use rosc::{OscMessage, OscPacket, OscType};

use crate::overlays::{keyboard::KEYBOARD_NAME, watch::WATCH_NAME};

use crate::backend::input::TrackedDeviceRole;

use super::{common::OverlayContainer, input::TrackedDevice};

pub struct OscSender {
    last_sent_overlay: Instant,
    last_sent_battery: Instant,
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
            last_sent_battery: Instant::now(),
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

    pub fn send_params<D>(
        &mut self,
        overlays: &OverlayContainer<D>,
        devices: &Vec<TrackedDevice>,
    ) -> anyhow::Result<()>
    where
        D: Default,
    {
        // send overlay data every 0.1 seconds
        if self.last_sent_overlay.elapsed().as_millis() >= 100 {
            self.last_sent_overlay = Instant::now();

            let mut num_overlays = 0;
            let mut has_keyboard = false;
            let mut has_wrist = false;

            for o in overlays.iter() {
                if !o.state.want_visible {
                    continue;
                }
                match o.state.name.as_ref() {
                    WATCH_NAME => has_wrist = true,
                    KEYBOARD_NAME => has_keyboard = true,
                    _ => {
                        if o.state.interactable {
                            num_overlays += 1;
                        }
                    }
                }
            }

            self.send_message(
                "/avatar/parameters/isOverlayOpen".into(),
                vec![OscType::Bool(num_overlays > 0)],
            )?;
            self.send_message(
                "/avatar/parameters/isKeyboardOpen".into(),
                vec![OscType::Bool(has_keyboard)],
            )?;
            self.send_message(
                "/avatar/parameters/isWristVisible".into(),
                vec![OscType::Bool(has_wrist)],
            )?;
            self.send_message(
                "/avatar/parameters/openOverlayCount".into(),
                vec![OscType::Int(num_overlays)],
            )?;
        }

        // send battery levels every 10 seconds
        if self.last_sent_battery.elapsed().as_millis() >= 10000 {
            self.last_sent_battery = Instant::now();

            let mut tracker_count: i8 = 0;
            let mut controller_count: i8 = 0;
            let mut tracker_total_bat = 0.0;
            let mut controller_total_bat = 0.0;

            for device in devices {
                let tracker_param;

                // soc is the battery level (set to device status.charge)
                let level = device.soc.unwrap_or(-1.0);
                let parameter = match device.role {
                    TrackedDeviceRole::None => continue,
                    TrackedDeviceRole::Hmd => {
                        // as of version 28-07-2025 (UI Update), OVR Toolkit now uses float 0-1
                        self.send_message(
                            "/avatar/parameters/hmdBattery".into(),
                            vec![OscType::Int((level * 100.0f32).round() as i32)],
                        )?;

                        "headset"
                    }
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

            // send average controller and tracker battery parameters
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
        }

        Ok(())
    }

    pub fn send_single_param(
        &mut self,
        parameter: String,
        values: Vec<OscType>,
    ) -> anyhow::Result<()> {
        self.send_message(parameter, values)?;

        Ok(())
    }
}
