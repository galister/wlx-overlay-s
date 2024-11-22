use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket},
    time::Instant,
};

use anyhow::bail;
use rosc::{OscMessage, OscPacket, OscType};

use crate::overlays::{keyboard::KEYBOARD_NAME, watch::WATCH_NAME};

use crate::{
    backend::input::TrackedDeviceRole,
    state::AppState,
};

use super::common::OverlayContainer;

pub struct OscSender {
    last_sent: Instant,
    upstream: UdpSocket,
}

impl OscSender {
    pub fn new(send_port: u16) -> anyhow::Result<Self> {
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);

        let Ok(upstream) = UdpSocket::bind("0.0.0.0:0") else {
            bail!("Failed to bind UDP socket - OSC will not function.");
        };

        let Ok(_) = upstream.connect(SocketAddr::new(ip, send_port)) else {
            bail!("Failed to connect UDP socket - OSC will not function.");
        };

        Ok(Self {
            upstream,
            last_sent: Instant::now(),
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

    pub fn send_params<D>(&mut self, overlays: &OverlayContainer<D>, app: &AppState) -> anyhow::Result<()>
    where
        D: Default,
    {
        if self.last_sent.elapsed().as_millis() < 100 {
            return Ok(());
        }

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
                        num_overlays += 1
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

        // battery levels
        let mut tracker_idx = 0;

        for device in &app.input_state.devices {

            // i believe soc is the battery level, based on input.rs (status.charge)
            let level = device.soc.unwrap();
            let mut parameter = "";

            match device.role {
                TrackedDeviceRole::None =>      {}
                TrackedDeviceRole::Hmd =>       {
                    // XSOverlay style (float)
                    // this parameter doesn't exist, but it's a stepping stone for 0-1 values (i presume XSOverlay would use the full name headset and not the abbreviation hmd)
                    parameter = "headsetBattery";

                    // legacy OVR Toolkit style (int)
                    // according to their docs, OVR Toolkit is now supposed to use float 0-1.
                    // as of 20 Nov 2024 they still use int 0-100, but this may change in a future update.
                    //TODO: remove once their implementation matches the docs
                    self.send_message(
                        "/avatar/parameters/hmdBattery".into(),
                                      vec![OscType::Int((level * 100.0f32).round() as i32)],
                    )?;

                }
                TrackedDeviceRole::LeftHand =>  {parameter = "leftControllerBattery"}
                TrackedDeviceRole::RightHand => {parameter = "rightControllerBattery"}
                TrackedDeviceRole::Tracker =>   {
                    //TODO: the String gets dropped i presume once this block exits (so parameter becomes a null ref), even if i set owner.
                    //parameter = format!("tracker{tracker_idx}Battery").as_str();
                    //            ^^^^^^ "temporary value dropped" ^^^^^

                    //TODO: figure out how to put the number in the string properly as above (which doesn't work)
                    parameter = "tracker";
                    tracker_idx += 1;
                }
            }

            // send level parameter
            if !parameter.is_empty() {
                //TODO: figure out how to put the number in the string, ideally in the TrackedDeviceRole section where we set the parameters
                if parameter == "tracker" {
                    self.send_message(
                        format!("/avatar/parameters/tracker{tracker_idx}Battery").into(),
                                    vec![OscType::Float(level)],
                    )?;
                }

                else {
                    self.send_message(
                        format!("/avatar/parameters/{parameter}").into(),
                                    vec![OscType::Float(level)],
                    )?;
                }
            }
        }
        

        Ok(())
    }
}
