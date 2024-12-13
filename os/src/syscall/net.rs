use alloc::sync::Arc;
use lose_net_stack::IPv4;

use crate::{
    net::{
        net_interrupt_handler,
        port_table::{accept, listen, port_acceptable, PortFd},
        udp::UDP,
    },
    task::{current_process, current_task, current_trap_cx},
};

/// udp only
pub fn sys_connect(raddr: u32, lport: u16, rport: u16) -> isize {
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    let fd = inner.alloc_fd();
    let udp_node = UDP::new(IPv4::from_u32(raddr), lport, rport);
    inner.fd_table[fd] = Some(Arc::new(udp_node));
    fd as isize
}

pub fn sys_listen(port: u16) -> isize {
    let port_idx = match listen(port) {
        Some(v) => v,
        _ => return -1,
    };

    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    let fd = inner.alloc_fd();
    let port_fd = PortFd::new(port_idx);
    inner.fd_table[fd] = Some(Arc::new(port_fd));
    port_idx as isize // port index NOT fd
}

pub fn sys_accept(port_idx: usize) -> isize {
    let task = current_task().unwrap();
    accept(port_idx, task);

    loop {
        net_interrupt_handler();
        if !port_acceptable(port_idx) {
            break;
        }
    }

    let trap_cx = current_trap_cx();
    trap_cx.x[10] as isize
}
