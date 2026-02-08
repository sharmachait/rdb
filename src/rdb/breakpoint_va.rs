use crate::rdb::{process::Process, virtual_address::VirtAddr};

pub struct BreakpointVA {
    id: usize,
    virtual_address: VirtAddr,
    is_enabled_: bool,
    pub instruction_replaced: u8,
}

impl BreakpointVA {
    pub fn get_id(&self) -> usize {
        self.id
    }
    pub fn create_for_process_at(process: &Process, addr: VirtAddr) -> Self {
        Self {
            virtual_address: addr,
            is_enabled_: false,
            instruction_replaced: 0,
            id: process.breakpoints.len() + 1, // we will hardly ever remove a breakpoint, mostly
                                               // will disable it
        }
    }
    pub fn enable(&mut self) {
        self.is_enabled_ = true;
    }
    pub fn disable(&mut self) {
        self.is_enabled_ = false;
    }
    pub fn is_enabled(&self) -> bool {
        self.is_enabled_
    }
    pub fn address(&self) -> &VirtAddr {
        &self.virtual_address
    }
    pub fn is_at(&self, addr: &VirtAddr) -> bool {
        self.virtual_address == *addr
    }
    pub fn is_in_range_high_exclusive(&self, low: &VirtAddr, high: &VirtAddr) -> bool {
        *low <= self.virtual_address && *high > self.virtual_address
    }
}
