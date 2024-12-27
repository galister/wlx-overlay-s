use interprocess::local_socket::{self, traits::Listener, ToNsName};
use smallvec::SmallVec;
use std::io::Read;

use crate::backend::wayvr::wlx_server_ipc::ipc;

pub struct Connection {
    alive: bool,
    conn: local_socket::Stream,
    next_packet_size: Option<u32>,
    handshaking: bool,
}

fn read_check(expected_size: u32, res: std::io::Result<usize>) -> bool {
    match res {
        Ok(count) => {
            if count == 0 {
                return false;
            }
            if count as u32 != expected_size {
                log::error!("count {} is not {}", count, expected_size);
                false
            } else {
                true // read succeeded
            }
        }
        Err(e) => {
            log::error!("failed to get packet size: {}", e);
            false
        }
    }
}

type Payload = SmallVec<[u8; 64]>;

fn read_payload(conn: &mut local_socket::Stream, size: u32) -> Option<Payload> {
    let mut payload = Payload::new();
    payload.resize(size as usize, 0);
    if !read_check(size, conn.read(&mut payload)) {
        None
    } else {
        Some(payload)
    }
}

impl Connection {
    fn new(conn: local_socket::Stream) -> Self {
        Self {
            conn,
            alive: true,
            handshaking: true,
            next_packet_size: None,
        }
    }

    fn kill(&mut self) {
        self.alive = false;
    }

    fn process_payload(&mut self, payload: Payload) {
        if self.handshaking {
            let handshake: ipc::Handshake = match postcard::from_bytes(&payload) {
                Ok(o) => o,
                Err(e) => {
                    log::error!("Invalid packet: {}", e);
                    self.kill();
                    return;
                }
            };

            if handshake.protocol_version != ipc::PROTOCOL_VERSION {
                log::error!(
                    "Unsupported protocol version {}",
                    handshake.protocol_version
                );
                self.kill();
            }

            if handshake.magic != ipc::CONNECTION_MAGIC {
                log::error!("Invalid magic");
                self.kill();
            }

            log::info!("Accepted new connection");
            self.handshaking = false;
        }
    }

    fn read_packet(&mut self) -> bool {
        if let Some(next_packet_size) = self.next_packet_size {
            let Some(payload) = read_payload(&mut self.conn, next_packet_size) else {
                // still failed to read payload, try in next tick
                return false;
            };

            self.process_payload(payload);
            self.next_packet_size = None;
        }

        let mut buf_packet_size: [u8; 4] = [0; 4];
        if !read_check(4, self.conn.read(&mut buf_packet_size)) {
            return false;
        }

        let packet_size = u32::from_be_bytes(buf_packet_size);

        if packet_size > 128 * 1024 {
            // over 128 KiB?
            self.kill();
            return false;
        }

        let Some(payload) = read_payload(&mut self.conn, packet_size) else {
            // failed to read payload, try in next tick
            self.next_packet_size = Some(packet_size);
            return false;
        };

        self.process_payload(payload);
        true
    }

    fn tick(&mut self) {
        while self.read_packet() {}
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        log::info!("Connection closed");
    }
}

pub struct WayVRServer {
    listener: local_socket::Listener,
    connections: Vec<Connection>,
}

impl WayVRServer {
    pub fn new() -> anyhow::Result<Self> {
        let printname = "wlx_dashboard_ipc.sock";
        let name = printname.to_ns_name::<local_socket::GenericNamespaced>()?;
        let opts = local_socket::ListenerOptions::new()
            .name(name)
            .nonblocking(local_socket::ListenerNonblockingMode::Both);
        let listener = match opts.create_sync() {
            Ok(listener) => listener,
            Err(e) => anyhow::bail!("Failed to start WayVRServer IPC listener. Reason: {}", e),
        };

        log::info!("WayVRServer IPC running at {}", printname);

        Ok(Self {
            listener,
            connections: Vec::new(),
        })
    }

    fn accept_connections(&mut self) {
        let Ok(conn) = self.listener.accept() else {
            return; // No new connection or other error
        };

        self.connections.push(Connection::new(conn));
    }

    fn tick_connections(&mut self) {
        for c in &mut self.connections {
            c.tick();
        }

        // remove killed connections
        self.connections.retain(|c| c.alive);
    }

    pub fn tick(&mut self) -> anyhow::Result<()> {
        self.accept_connections();
        self.tick_connections();
        Ok(())
    }
}
