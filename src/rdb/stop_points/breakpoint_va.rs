use crate::rdb::stop_points::stoppoint_collection::Stoppoint;
use crate::rdb::{process::Process, virtual_address::VirtAddr};

pub struct BreakpointVA {
    id: usize,
    virtual_address: VirtAddr,
    is_enabled_: bool,
    pub instruction_replaced: u8,
}

impl BreakpointVA {
    pub fn create_for_process_at(process: &mut Process, addr: VirtAddr) -> Self {
        process.breakpoint_id = process.breakpoint_id + 1;
        Self {
            virtual_address: addr,
            is_enabled_: false,
            instruction_replaced: 0,
            id: process.breakpoint_id,
        }
    }
    pub fn enable(&mut self) {
        self.is_enabled_ = true;
    }
    pub fn is_at(&self, addr: &VirtAddr) -> bool {
        self.virtual_address == *addr
    }
    pub fn is_in_range_high_exclusive(&self, low: &VirtAddr, high: &VirtAddr) -> bool {
        *low <= self.virtual_address && *high > self.virtual_address
    }
}

impl Stoppoint for BreakpointVA {
    fn id(&self) -> usize {
        self.id
    }
    fn address(&self) -> &VirtAddr {
        &self.virtual_address
    }
    fn is_enabled(&self) -> bool {
        self.is_enabled_
    }
    fn disable(&mut self) {
        self.is_enabled_ = false;
    }
}
