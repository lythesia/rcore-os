use riscv::register::sstatus::{self, Sstatus, SPP};

#[repr(C)]
pub struct TrapContext {
    /// reg: x0 ~ x31
    pub x: [usize; 32],
    /// csr: sstatus
    pub sstatus: Sstatus,
    /// csr: sepc
    pub sepc: usize,

    // 以下字段在应用初始化的时候由内核写入应用地址空间中的 TrapContext 的相应位置，此后就不再被修改
    /// 内核地址空间的 token ，即内核页表的起始物理地址
    pub kernel_satp: usize,
    /// 当前应用在内核地址空间中的内核栈栈顶的虚拟地址
    pub kernel_sp: usize,
    ///  内核中 trap handler 入口点的虚拟地址
    pub trap_handler: usize,
}

impl TrapContext {
    pub fn set_sp(&mut self, sp: usize) {
        self.x[2] = sp;
    }

    pub fn app_init_context(
        entry: usize,
        sp: usize,
        kernel_satp: usize,
        kernel_sp: usize,
        trap_handler: usize,
    ) -> Self {
        let mut sstatus = sstatus::read();
        sstatus.set_spp(SPP::User);
        let mut cx = Self {
            x: [0; 32],
            sstatus,
            sepc: entry,
            kernel_satp,
            kernel_sp,
            trap_handler,
        };
        cx.set_sp(sp);
        cx
    }
}