use dbus::{
    blocking::Connection,
    channel::{MatchingReceiver, Token},
    message::MatchRule,
};
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
    dbus_data: Option<(Connection, Token)>,
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
        if let Some((c, _)) = &self.dbus_data {
            let _ = c.process(Duration::ZERO);
        }

        self.rx_toast.try_iter().for_each(|toast| {
            toast.submit(app);
        });
    }

    pub fn run_dbus(&mut self) {
        let Ok(c) = Connection::new_session() else {
            log::error!("Failed to connect to dbus. Desktop notifications will not work.");
            return;
        };

        let mut rule = MatchRule::new_method_call();
        rule.member = Some("Notify".into());
        rule.interface = Some("org.freedesktop.Notifications".into());
        rule.path = Some("/org/freedesktop/Notifications".into());
        rule.eavesdrop = true;

        let sender = self.tx_toast.clone();

        let Ok(token) = c.add_match(rule, move |_: (), _, msg| {
            if let Ok(toast) = parse_dbus(&msg) {
                match sender.try_send(toast) {
                    Ok(_) => {}
                    Err(e) => {
                        log::error!("Failed to send notification: {:?}", e);
                    }
                }
            }
            true
        }) else {
            log::error!("Failed to add dbus match. Desktop notifications will not work.");
            return;
        };

        self.dbus_data = Some((c, token));
    }

    pub fn run_udp(&mut self) {
        let sender = self.tx_toast.clone();
        // NOTE: We're detaching the thread, as there's no simple way to gracefully stop it other than app shutdown.
        let _ = std::thread::spawn(move || {
            let socket = match std::net::UdpSocket::bind("127.0.0.1:42069") {
                Ok(s) => s,
                Err(e) => {
                    log::error!("Failed to bind notification socket: {:?}", e);
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
                        .with_sound(msg.volume.unwrap_or(0.) > 0.1);

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

impl Drop for NotificationManager {
    fn drop(&mut self) {
        if let Some((c, token)) = self.dbus_data.take() {
            let _ = c.stop_receive(token);
        }
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
        .with_sound(true)
        .with_timeout(5.0)
        .with_opacity(1.0))
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
