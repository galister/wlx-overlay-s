use std::time::Duration;

use anyhow::Context;
use dbus::{
    Message,
    arg::{PropMap, Variant},
    blocking::Connection,
    channel::MatchingReceiver,
    message::MatchRule,
};

use crate::subsystem::dbus::notifications::OrgFreedesktopNotifications;

mod notifications;

pub type DbusReceiveCallback = Box<dyn FnMut(Message, &Connection) -> bool + Send>;
pub type DbusMatchCallback = Box<dyn FnMut((), &Connection, &Message) -> bool + Send>;

#[derive(Default)]
pub struct DbusConnector {
    pub connection: Option<Connection>,
}

impl DbusConnector {
    pub fn tick(&self) {
        if let Some(c) = self.connection.as_ref() {
            let _ = c.process(Duration::ZERO);
        }
    }

    pub fn become_monitor(
        &mut self,
        rule: MatchRule<'static>,
        callback: DbusReceiveCallback,
    ) -> anyhow::Result<()> {
        let connection = self
            .connection
            .take()
            .context("Not connected")
            .or_else(|_| Connection::new_session())?;

        let proxy = connection.with_proxy(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            Duration::from_millis(5000),
        );
        let result: Result<(), dbus::Error> = proxy.method_call(
            "org.freedesktop.DBus.Monitoring",
            "BecomeMonitor",
            (vec![rule.match_str()], 0u32),
        );

        result?;

        let _ = connection.start_receive(rule, callback);

        self.connection = Some(connection);
        Ok(())
    }

    pub fn add_match(
        &mut self,
        rule: MatchRule<'static>,
        callback: DbusMatchCallback,
    ) -> anyhow::Result<()> {
        let connection = self
            .connection
            .take()
            .context("Not connected")
            .or_else(|_| Connection::new_session())?;

        let _ = connection.add_match(rule, callback)?;
        self.connection = Some(connection);
        Ok(())
    }

    pub fn notify_send(
        &mut self,
        summary: &str,
        body: &str,
        urgency: u8,
        timeout: i32,
        replaces_id: u32,
        transient: bool,
    ) -> anyhow::Result<u32> {
        let connection = self
            .connection
            .take()
            .context("Not connected")
            .or_else(|_| Connection::new_session())?;

        let proxy = connection.with_proxy(
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            Duration::from_millis(1000),
        );

        let mut hints = PropMap::new();
        hints.insert("urgency".to_string(), Variant(Box::new(urgency)));
        hints.insert("transient".to_string(), Variant(Box::new(transient)));

        let retval = proxy.notify(
            "WlxOverlay-S",
            replaces_id,
            "",
            summary,
            body,
            vec![],
            hints,
            timeout,
        )?;
        self.connection = Some(connection);

        Ok(retval)
    }

    pub fn notify_close(&mut self, id: u32) -> anyhow::Result<()> {
        let connection = self
            .connection
            .take()
            .context("Not connected")
            .or_else(|_| Connection::new_session())?;
        let proxy = connection.with_proxy(
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            Duration::from_millis(1000),
        );

        proxy.close_notification(id)?;
        self.connection = Some(connection);
        Ok(())
    }
}
