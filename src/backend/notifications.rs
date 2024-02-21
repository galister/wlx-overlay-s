use dbus::{blocking::Connection, channel::MatchingReceiver, message::MatchRule};
use serde::Deserialize;
use std::{
    sync::{
        mpsc::{self},
        Arc,
    },
    time::Duration,
};

use crate::{overlays::toast::Toast, state::AppState};

pub struct NotificationManager {
    rx_toast: mpsc::Receiver<Toast>,
    tx_toast: mpsc::SyncSender<Toast>,
    dbus_data: Option<Connection>,
}

impl NotificationManager {
    pub fn new() -> Self {
        let (tx_toast, rx_toast) = mpsc::sync_channel(10);
        Self {
            rx_toast,
            tx_toast,
            dbus_data: None,
        }
    }

    pub fn submit_pending(&self, app: &mut AppState) {
        if let Some(c) = &self.dbus_data {
            let _ = c.process(Duration::ZERO);
        }

        self.rx_toast.try_iter().for_each(|toast| {
            toast.submit(app);
        });
    }

    pub fn run_dbus(&mut self) {
        let c = match Connection::new_session() {
            Ok(c) => c,
            Err(e) => {
                log::error!(
                    "Failed to connect to dbus. Desktop notifications will not work. Cause: {:?}",
                    e
                );
                return;
            }
        };

        let mut rule = MatchRule::new_method_call();
        rule.member = Some("Notify".into());
        rule.interface = Some("org.freedesktop.Notifications".into());
        rule.path = Some("/org/freedesktop/Notifications".into());
        rule.eavesdrop = true;

        let proxy = c.with_proxy(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            Duration::from_millis(5000),
        );
        let result: Result<(), dbus::Error> = proxy.method_call(
            "org.freedesktop.DBus.Monitoring",
            "BecomeMonitor",
            (vec![rule.match_str()], 0u32),
        );

        match result {
            Ok(_) => {
                let sender = self.tx_toast.clone();
                c.start_receive(
                    rule,
                    Box::new(move |msg, _| {
                        if let Ok(toast) = parse_dbus(&msg) {
                            match sender.try_send(toast) {
                                Ok(_) => {}
                                Err(e) => {
                                    log::error!("Failed to send notification: {:?}", e);
                                }
                            }
                        }
                        true
                    }),
                );
                log::info!("Listening to DBus notifications via BecomeMonitor.");
            }
            Err(_) => {
                let rule_with_eavesdrop = {
                    let mut rule = rule.clone();
                    rule.eavesdrop = true;
                    rule
                };

                let sender2 = self.tx_toast.clone();
                let result = c.add_match(rule_with_eavesdrop, move |_: (), _, msg| {
                    if let Ok(toast) = parse_dbus(&msg) {
                        match sender2.try_send(toast) {
                            Ok(_) => {}
                            Err(e) => {
                                log::error!("Failed to send notification: {:?}", e);
                            }
                        }
                    }
                    true
                });

                match result {
                    Ok(_) => {
                        log::info!("Listening to DBus notifications via eavesdrop.");
                    }
                    Err(_) => {
                        log::error!(
                            "Failed to add DBus match. Desktop notifications will not work.",
                        );
                    }
                }
            }
        }

        self.dbus_data = Some(c);
    }

    pub fn run_udp(&mut self) {
        let sender = self.tx_toast.clone();
        // NOTE: We're detaching the thread, as there's no simple way to gracefully stop it other than app shutdown.
        let _ = std::thread::spawn(move || {
            let addr = "127.0.0.1:42069";
            let socket = match std::net::UdpSocket::bind(addr) {
                Ok(s) => s,
                Err(e) => {
                    log::error!("Failed to bind notification socket @ {}: {:?}", addr, e);
                    return;
                }
            };
            let mut buf = [0u8; 1024 * 16]; // vrcx embeds icons as b64

            loop {
                if let Ok((num_bytes, _)) = socket.recv_from(&mut buf) {
                    let json_str = match std::str::from_utf8(&buf[..num_bytes]) {
                        Ok(s) => s,
                        Err(e) => {
                            log::error!("Failed to receive notification message: {:?}", e);
                            continue;
                        }
                    };
                    log::info!("Received notification message: {}", json_str);
                    let msg = match serde_json::from_str::<XsoMessage>(json_str) {
                        Ok(m) => m,
                        Err(e) => {
                            log::error!("Failed to parse notification message: {:?}", e);
                            continue;
                        }
                    };

                    if msg.messageType != 1 {
                        continue;
                    }

                    let toast = Toast::new(msg.title, msg.content.unwrap_or_else(|| "".into()))
                        .with_timeout(msg.timeout.unwrap_or(5.))
                        .with_sound(msg.volume.unwrap_or(-1.) >= 0.); // XSOverlay still plays at 0,

                    match sender.try_send(toast) {
                        Ok(_) => {}
                        Err(e) => {
                            log::error!("Failed to send notification: {:?}", e);
                        }
                    }
                }
            }
        });
    }
}

fn parse_dbus(msg: &dbus::Message) -> anyhow::Result<Toast> {
    let mut args = msg.iter_init();
    let app_name: String = args.read()?;
    let _replaces_id: u32 = args.read()?;
    let _app_icon: String = args.read()?;
    let summary: String = args.read()?;
    let body: String = args.read()?;

    let title = if summary.is_empty() {
        app_name
    } else {
        summary
    };

    Ok(Toast::new(title.into(), body.into())
        .with_timeout(5.0)
        .with_opacity(1.0))
    // leave the audio part to the desktop env
}

#[allow(non_snake_case)]
#[derive(Debug, Deserialize)]
struct XsoMessage {
    messageType: i32,
    index: Option<i32>,
    volume: Option<f32>,
    audioPath: Option<Arc<str>>,
    timeout: Option<f32>,
    title: Arc<str>,
    content: Option<Arc<str>>,
    icon: Option<Arc<str>>,
    height: Option<f32>,
    opacity: Option<f32>,
    useBase64Icon: Option<bool>,
    sourceApp: Option<Arc<str>>,
    alwaysShow: Option<bool>,
}
