use alloc::{collections::VecDeque, vec::Vec};
use lazy_static::lazy_static;
use lose_net_stack::IPv4;

use crate::sync::UPIntrFreeCell;

pub struct Socket {
    // remote addr
    pub raddr: IPv4,
    // local port
    pub lport: u16,
    // remote port
    pub rport: u16,
    // data frames
    pub buffers: VecDeque<Vec<u8>>,
    // pack seq
    pub seq: u32,
    // pack ack
    pub ack: u32,
}

lazy_static! {
    static ref SOCKET_TABLE: UPIntrFreeCell<Vec<Option<Socket>>> =
        unsafe { UPIntrFreeCell::new(Vec::new()) };
}

/// get `seq & ack` by socket index
pub fn get_sa_by_index(idx: usize) -> Option<(u32, u32)> {
    let socket_table = SOCKET_TABLE.exclusive_access();
    assert!(idx < socket_table.len());
    socket_table
        .get(idx)
        .map(|x| x.as_ref().map(|y| (y.seq, y.ack)))
        .flatten()
}

/// set `seq & ack` by socket index
pub fn set_sa_by_index(idx: usize, seq: u32, ack: u32) {
    let mut socket_table = SOCKET_TABLE.exclusive_access();
    assert!(idx < socket_table.len());
    let sock = socket_table[idx].as_mut().expect("sock not exist");
    sock.seq = seq;
    sock.ack = ack;
}

pub fn get_socket(raddr: IPv4, lport: u16, rport: u16) -> Option<usize> {
    let socket_table = SOCKET_TABLE.exclusive_access();
    for (i, slot) in socket_table.iter().enumerate() {
        match &slot {
            Some(sock) => {
                if sock.raddr == raddr && sock.lport == lport && sock.rport == rport {
                    return Some(i);
                }
            }
            _ => continue,
        }
    }
    None
}

pub fn add_socket(raddr: IPv4, lport: u16, rport: u16) -> Option<usize> {
    if get_socket(raddr, lport, rport).is_some() {
        return None;
    }

    let sock = Socket {
        raddr,
        lport,
        rport,
        buffers: VecDeque::new(),
        seq: 0,
        ack: 0,
    };
    let mut socket_table = SOCKET_TABLE.exclusive_access();
    match socket_table.iter().position(Option::is_none) {
        Some(pos) => {
            socket_table[pos] = Some(sock);
            Some(pos)
        }
        _ => {
            socket_table.push(Some(sock));
            Some(socket_table.len() - 1)
        }
    }
}

pub fn remove_socket(idx: usize) {
    let mut socket_table = SOCKET_TABLE.exclusive_access();
    assert!(idx < socket_table.len());
    socket_table[idx] = None;
}

pub fn push_data(idx: usize, data: Vec<u8>) {
    let mut socket_table = SOCKET_TABLE.exclusive_access();
    assert!(idx < socket_table.len());
    let sock = socket_table[idx].as_mut().expect("sock not exist");
    sock.buffers.push_back(data);
}

pub fn pop_data(idx: usize) -> Option<Vec<u8>> {
    let mut socket_table = SOCKET_TABLE.exclusive_access();
    assert!(idx < socket_table.len());
    let sock = socket_table[idx].as_mut().expect("sock not exist");
    sock.buffers.pop_front()
}
