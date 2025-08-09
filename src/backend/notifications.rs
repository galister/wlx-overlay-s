use dbus::{
    arg::{PropMap, Variant},
    blocking::Connection,
    channel::MatchingReceiver,
    message::MatchRule,
};
use serde::Deserialize;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::Duration,
};

use crate::{
    backend::notifications_dbus::OrgFreedesktopNotifications,
    overlays::toast::{Toast, ToastTopic},
    state::AppState,
};

pub struct NotificationManager {
    rx_toast: mpsc::Receiver<Toast>,
    tx_toast: mpsc::SyncSender<Toast>,
    dbus_data: Option<Connection>,
    running: Arc<AtomicBool>,
}

impl NotificationManager {
    pub fn new() -> Self {
        let (tx_toast, rx_toast) = mpsc::sync_channel(10);
        Self {
            rx_toast,
            tx_toast,
            dbus_data: None,
            running: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn submit_pending(&self, app: &mut AppState) {
        if let Some(c) = &self.dbus_data {
            let _ = c.process(Duration::ZERO);
        }

        if app.session.config.notifications_enabled {
            self.rx_toast.try_iter().for_each(|toast| {
                toast.submit(app);
            });
        } else {
            // consume without submitting
            self.rx_toast.try_iter().last();
        }
    }

    pub fn run_dbus(&mut self) {
        let c = match Connection::new_session() {
            Ok(c) => c,
            Err(e) => {
                log::error!(
                    "Failed to connect to dbus. Desktop notifications will not work. Cause: {e:?}"
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

        if matches!(result, Ok(())) {
            let sender = self.tx_toast.clone();
            c.start_receive(
                rule,
                Box::new(move |msg, _| {
                    if let Ok(toast) = parse_dbus(&msg) {
                        match sender.try_send(toast) {
                            Ok(()) => {}
                            Err(e) => {
                                log::error!("Failed to send notification: {e:?}");
                            }
                        }
                    }
                    true
                }),
            );
            log::info!("Listening to DBus notifications via BecomeMonitor.");
        } else {
            let rule_with_eavesdrop = {
                let mut rule = rule.clone();
                rule.eavesdrop = true;
                rule
            };

            let sender2 = self.tx_toast.clone();
            let result = c.add_match(rule_with_eavesdrop, move |(): (), _, msg| {
                if let Ok(toast) = parse_dbus(msg) {
                    match sender2.try_send(toast) {
                        Ok(()) => {}
                        Err(e) => {
                            log::error!("Failed to send notification: {e:?}");
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
                    log::error!("Failed to add DBus match. Desktop notifications will not work.",);
                }
            }
        }

        self.dbus_data = Some(c);
    }

    pub fn run_udp(&mut self) {
        let sender = self.tx_toast.clone();
        let running = self.running.clone();
        let _ = std::thread::spawn(move || {
            let addr = "127.0.0.1:42069";
            let socket = match std::net::UdpSocket::bind(addr) {
                Ok(s) => s,
                Err(e) => {
                    log::error!("Failed to bind notification socket @ {addr}: {e:?}");
                    return;
                }
            };
            if let Err(err) = socket.set_read_timeout(Some(Duration::from_millis(200))) {
                log::error!("Failed to set read timeout: {err:?}");
            }

            let mut buf = [0u8; 1024 * 16]; // vrcx embeds icons as b64

            while running.load(Ordering::Relaxed) {
                if let Ok((num_bytes, _)) = socket.recv_from(&mut buf) {
                    let json_str = match std::str::from_utf8(&buf[..num_bytes]) {
                        Ok(s) => s,
                        Err(e) => {
                            log::error!("Failed to receive notification message: {e:?}");
                            continue;
                        }
                    };
                    let msg = match serde_json::from_str::<XsoMessage>(json_str) {
                        Ok(m) => m,
                        Err(e) => {
                            log::error!("Failed to parse notification message: {e:?}");
                            continue;
                        }
                    };

                    if msg.messageType != 1 {
                        continue;
                    }

                    let toast = Toast::new(
                        ToastTopic::XSNotification,
                        msg.title,
                        msg.content.unwrap_or_else(|| "".into()),
                    )
                    .with_timeout(msg.timeout.unwrap_or(5.))
                    .with_sound(msg.volume.unwrap_or(-1.) >= 0.); // XSOverlay still plays at 0,

                    match sender.try_send(toast) {
                        Ok(()) => {}
                        Err(e) => {
                            log::error!("Failed to send notification: {e:?}");
                        }
                    }
                }
            }
            log::info!("Notification listener stopped.");
        });
    }
}

impl Drop for NotificationManager {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

pub struct DbusNotificationSender {
    connection: Connection,
}

impl DbusNotificationSender {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            connection: Connection::new_session()?,
        })
    }

    pub fn notify_send(
        &self,
        summary: &str,
        body: &str,
        urgency: u8,
        timeout: i32,
        replaces_id: u32,
        transient: bool,
    ) -> anyhow::Result<u32> {
        let proxy = self.connection.with_proxy(
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            Duration::from_millis(1000),
        );

        let mut hints = PropMap::new();
        hints.insert("urgency".to_string(), Variant(Box::new(urgency)));
        hints.insert("transient".to_string(), Variant(Box::new(transient)));

        Ok(proxy.notify(
            "WlxOverlay-S",
            replaces_id,
            "",
            summary,
            body,
            vec![],
            hints,
            timeout,
        )?)
    }

    pub fn notify_close(&self, id: u32) -> anyhow::Result<()> {
        let proxy = self.connection.with_proxy(
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            Duration::from_millis(1000),
        );

        proxy.close_notification(id)?;
        Ok(())
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

    Ok(
        Toast::new(ToastTopic::DesktopNotification, title.into(), body.into())
            .with_timeout(5.0)
            .with_opacity(1.0),
    )
    // leave the audio part to the desktop env
}

#[allow(dead_code)]
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
