use bytes::BufMut;
use interprocess::local_socket::{self, traits::Listener, ToNsName};
use smallvec::SmallVec;
use std::io::{Read, Write};

use crate::backend::wayvr::wlx_server_ipc::ipc::{self, binary_decode};

use super::{
    display,
    wlx_server_ipc::{
        ipc::{binary_encode, Serial},
        packet_client::PacketClient,
        packet_server::{self, PacketServer},
    },
};

pub struct Connection {
    alive: bool,
    conn: local_socket::Stream,
    next_packet: Option<u32>,
    handshaking: bool,
}

pub fn send_packet(conn: &mut local_socket::Stream, data: &[u8]) -> anyhow::Result<()> {
    let mut bytes = bytes::BytesMut::new();

    // packet size
    bytes.put_u32(data.len() as u32);

    // packet data
    bytes.put_slice(data);

    conn.write_all(&bytes)?;

    Ok(())
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
        Err(_e) => {
            //log::error!("failed to get packet size: {}", e);
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

pub struct TickParams<'a> {
    pub displays: &'a super::display::DisplayVec,
}

impl Connection {
    fn new(conn: local_socket::Stream) -> Self {
        Self {
            conn,
            alive: true,
            handshaking: true,
            next_packet: None,
        }
    }

    fn kill(&mut self) {
        self.alive = false;
    }

    fn process_handshake(&mut self, payload: Payload) -> anyhow::Result<()> {
        let Ok(handshake) = binary_decode::<ipc::Handshake>(&payload) else {
            anyhow::bail!("Invalid handshake");
        };

        if handshake.protocol_version != ipc::PROTOCOL_VERSION {
            anyhow::bail!(
                "Unsupported protocol version {}",
                handshake.protocol_version
            );
        }

        if handshake.magic != ipc::CONNECTION_MAGIC {
            anyhow::bail!("Invalid magic");
        }

        log::info!("Accepted new connection");
        self.handshaking = false;
        Ok(())
    }

    fn process_list_displays(&mut self, params: &TickParams, serial: Serial) -> anyhow::Result<()> {
        let list: Vec<packet_server::Display> = params
            .displays
            .vec
            .iter()
            .enumerate()
            .filter_map(|(idx, opt_cell)| {
                let Some(cell) = opt_cell else {
                    return None;
                };
                let display = &cell.obj;
                Some(display.to_packet(display::DisplayHandle::new(idx as u32, cell.generation)))
            })
            .collect();

        send_packet(
            &mut self.conn,
            &binary_encode(&PacketServer::ListDisplaysResponse(
                serial,
                packet_server::DisplayList { list },
            )),
        )?;

        Ok(())
    }

    fn process_get_display(
        &mut self,
        params: &TickParams,
        serial: Serial,
        display_handle: packet_server::DisplayHandle,
    ) -> anyhow::Result<()> {
        let native_handle = &display::DisplayHandle::from_packet(display_handle.clone());
        let disp = params
            .displays
            .get(native_handle)
            .map(|disp| disp.to_packet(*native_handle));

        send_packet(
            &mut self.conn,
            &binary_encode(&PacketServer::GetDisplayResponse(serial, disp)),
        )?;

        Ok(())
    }

    fn process_payload(&mut self, params: &TickParams, payload: Payload) -> anyhow::Result<()> {
        if self.handshaking {
            self.process_handshake(payload)?;
            return Ok(());
        }

        let packet: PacketClient = binary_decode(&payload)?;
        match packet {
            PacketClient::ListDisplays(serial) => {
                self.process_list_displays(params, serial)?;
            }
            PacketClient::GetDisplay(serial, display_handle) => {
                self.process_get_display(params, serial, display_handle)?;
            }
        }

        Ok(())
    }

    fn process_check_payload(&mut self, params: &TickParams, payload: Payload) -> bool {
        log::debug!("payload size {}", payload.len());

        if let Err(e) = self.process_payload(params, payload) {
            log::error!("Invalid payload from the client, closing connection: {}", e);
            self.kill();
            false
        } else {
            true
        }
    }

    fn read_packet(&mut self, params: &TickParams) -> bool {
        if let Some(payload_size) = self.next_packet {
            let Some(payload) = read_payload(&mut self.conn, payload_size) else {
                // still failed to read payload, try in next tick
                return false;
            };

            if !self.process_check_payload(params, payload) {
                return false;
            }

            self.next_packet = None;
        }

        let mut buf_packet_header: [u8; 4] = [0; 4];
        if !read_check(4, self.conn.read(&mut buf_packet_header)) {
            return false;
        }

        let payload_size = u32::from_be_bytes(buf_packet_header[0..4].try_into().unwrap()); // 0-3 bytes (u32 size)

        let size_limit: u32 = 128 * 1024;

        if payload_size > size_limit {
            // over 128 KiB?
            log::error!(
                "Client sent a packet header with the size over {} bytes, closing connection.",
                size_limit
            );
            self.kill();
            return false;
        }

        let Some(payload) = read_payload(&mut self.conn, payload_size) else {
            // failed to read payload, try in next tick
            self.next_packet = Some(payload_size);
            return false;
        };

        if !self.process_check_payload(params, payload) {
            return false;
        }

        true
    }

    fn tick(&mut self, params: &TickParams) {
        while self.read_packet(params) {}
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

    fn tick_connections(&mut self, params: &TickParams) {
        for c in &mut self.connections {
            c.tick(params);
        }

        // remove killed connections
        self.connections.retain(|c| c.alive);
    }

    pub fn tick(&mut self, params: &TickParams) -> anyhow::Result<()> {
        self.accept_connections();
        self.tick_connections(params);
        Ok(())
    }
}
