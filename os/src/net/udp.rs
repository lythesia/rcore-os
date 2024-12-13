use alloc::vec;
use lose_net_stack::{packets::udp::UDPPacket, IPv4, MacAddress};

use crate::{drivers::NET_DEVICE, fs::File};

use super::{
    net_interrupt_handler,
    socket::{add_socket, pop_data, remove_socket},
    LOSE_NET_STACK,
};

pub struct UDP {
    pub target: IPv4,
    pub sport: u16,
    pub dport: u16,
    pub sock_idx: usize,
}

impl UDP {
    pub fn new(target: IPv4, sport: u16, dport: u16) -> Self {
        let sock_idx = add_socket(target, sport, dport).expect("can't add socket");
        Self {
            target,
            sport,
            dport,
            sock_idx,
        }
    }
}

impl File for UDP {
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

        let udp_packet = UDPPacket::new(
            lose_net_stack.ip,
            lose_net_stack.mac,
            self.sport,
            self.target,
            MacAddress::new([0xff, 0xff, 0xff, 0xff, 0xff, 0xff]),
            self.dport,
            total,
            data.as_ref(),
        );
        NET_DEVICE.transmit(&udp_packet.build_data());
        total
    }
}

impl Drop for UDP {
    fn drop(&mut self) {
        remove_socket(self.sock_idx);
    }
}
