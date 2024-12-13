use alloc::vec;
use lose_net_stack::{packets::tcp::TCPPacket, IPv4, MacAddress, TcpFlags};

use crate::{drivers::NET_DEVICE, fs::File};

use super::{
    net_interrupt_handler,
    socket::{add_socket, get_sa_by_index, pop_data, remove_socket},
    LOSE_NET_STACK,
};

#[derive(Debug)]
pub struct TCP {
    pub target: IPv4,
    pub sport: u16,
    pub dport: u16,
    #[allow(unused)]
    pub seq: u32,
    #[allow(unused)]
    pub ack: u32,
    pub sock_idx: usize,
}

impl TCP {
    pub fn new(target: IPv4, sport: u16, dport: u16, seq: u32, ack: u32) -> Self {
        let sock_idx = add_socket(target, sport, dport).expect("cannot add socket");
        Self {
            target,
            sport,
            dport,
            seq,
            ack,
            sock_idx,
        }
    }
}

impl File for TCP {
    fn readable(&self) -> bool {
        true
    }

    fn writable(&self) -> bool {
        true
    }

    fn read(&self, mut buf: crate::mm::UserBuffer) -> usize {
        loop {
            if let Some(data) = pop_data(self.sock_idx) {
                let total = data.len();
                let mut copied = 0;
                for b in buf.buffers.iter_mut() {
                    let to_copy = b.len().min(total - copied);
                    b[..to_copy].copy_from_slice(&data[copied..(copied + to_copy)]);
                    copied += to_copy;
                    if copied == total {
                        break;
                    }
                }
                return copied;
            } else {
                net_interrupt_handler();
            }
        }
    }

    fn write(&self, buf: crate::mm::UserBuffer) -> usize {
        let lose_net_stack = LOSE_NET_STACK.0.exclusive_access();

        let mut data = vec![0u8; buf.len()];

        let mut copied = 0;
        for b in buf.buffers.iter() {
            data[copied..(copied + b.len())].copy_from_slice(b);
            copied += b.len();
        }
        let total = data.len();

        let (ack, seq) = get_sa_by_index(self.sock_idx).unwrap_or((0, 0));
        let tcp_packet = TCPPacket {
            source_ip: lose_net_stack.ip,
            source_mac: lose_net_stack.mac,
            source_port: self.sport,
            dest_ip: self.target,
            dest_mac: MacAddress::new([0xff, 0xff, 0xff, 0xff, 0xff, 0xff]), // brd
            dest_port: self.dport,
            data_len: total,
            seq,
            ack,
            flags: TcpFlags::A,
            win: 65535,
            urg: 0,
            data: data.as_ref(),
        };
        NET_DEVICE.transmit(&tcp_packet.build_data());
        total
    }
}

impl Drop for TCP {
    fn drop(&mut self) {
        remove_socket(self.sock_idx);
    }
}
