use alloc::{sync::Arc, vec::Vec};
use lazy_static::lazy_static;
use lose_net_stack::packets::tcp::TCPPacket;

use crate::{fs::File, sync::UPIntrFreeCell, task::TaskControlBlock};

use super::tcp::TCP;

pub struct Port {
    pub port: u16,
    pub receivable: bool,
    pub schedule: Option<Arc<TaskControlBlock>>,
}

lazy_static! {
    static ref LISTEN_TABLE: UPIntrFreeCell<Vec<Option<Port>>> =
        unsafe { UPIntrFreeCell::new(Vec::new()) };
}

pub fn listen(port: u16) -> Option<usize> {
    let listen_port = Port {
        port,
        receivable: true,
        schedule: None,
    };
    let mut listen_table = LISTEN_TABLE.exclusive_access();
    match listen_table.iter().position(Option::is_none) {
        Some(pos) => {
            listen_table[pos] = Some(listen_port);
            return Some(pos);
        }
        _ => {
            listen_table.push(Some(listen_port));
            Some(listen_table.len() - 1)
        }
    }
}

pub fn accept(listen_idx: usize, task: Arc<TaskControlBlock>) {
    let mut listen_table = LISTEN_TABLE.exclusive_access();
    assert!(listen_idx < listen_table.len());
    let listen_port = listen_table[listen_idx]
        .as_mut()
        .expect("listen port not exist");
    listen_port.receivable = true;
    listen_port.schedule = Some(task);
}

pub fn port_acceptable(listen_idx: usize) -> bool {
    let listen_table = LISTEN_TABLE.exclusive_access();
    assert!(listen_idx < listen_table.len());
    listen_table[listen_idx]
        .as_ref()
        .map_or(false, |v| v.receivable)
}

pub fn check_accept(port: u16, tcp_packet: &TCPPacket) -> Option<()> {
    LISTEN_TABLE.exclusive_session(|listen_table| {
        let listen_port = listen_table
            .iter_mut()
            .flatten()
            .find(|v| v.port == port && v.receivable)?;
        let task = listen_port.schedule.take()?;
        listen_port.receivable = false;
        accept_connection(port, tcp_packet, task);
        Some(())
    })
}

pub fn accept_connection(_port: u16, tcp_packet: &TCPPacket, task: Arc<TaskControlBlock>) {
    let process = task.process.upgrade().unwrap();
    let mut inner = process.inner_exclusive_access();
    let fd = inner.alloc_fd();

    let tcp_socket = TCP::new(
        tcp_packet.source_ip,
        tcp_packet.dest_port,
        tcp_packet.source_port,
        tcp_packet.seq,
        tcp_packet.ack,
    );
    inner.fd_table[fd] = Some(Arc::new(tcp_socket));

    let trap_cx = task.inner_exclusive_access().get_trap_cx();
    trap_cx.x[10] = fd; // return fd
}

pub struct PortFd(usize);

impl PortFd {
    pub fn new(port_idx: usize) -> Self {
        Self(port_idx)
    }
}

impl Drop for PortFd {
    fn drop(&mut self) {
        // assert?
        LISTEN_TABLE.exclusive_access()[self.0] = None;
    }
}

impl File for PortFd {
    fn readable(&self) -> bool {
        false
    }

    fn writable(&self) -> bool {
        false
    }

    fn read(&self, _buf: crate::mm::UserBuffer) -> usize {
        0
    }

    fn write(&self, _buf: crate::mm::UserBuffer) -> usize {
        0
    }
}
