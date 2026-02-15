use crate::rdb::stop_points::stoppoint_collection::Stoppoint;
use crate::rdb::{process::Process, virtual_address::VirtAddr};
use nix::sys::ptrace;

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
    fn disable(&mut self, process: &Process) -> Result<(), String> {
        if !self.is_enabled_ {
            return Ok(());
        }
        let data = ptrace::read(
            process.pid(),
            self.virtual_address.addr() as usize as *mut _,
        )
        .map_err(|e| {
            format!(
                "Couldnt read adta at {}, {}",
                self.virtual_address.addr(),
                e
            )
        })?;
        let restored_data = (data & !0xff) | (self.instruction_replaced as i64);
        ptrace::write(
            process.pid(),
            self.virtual_address.addr() as usize as *mut _,
            restored_data,
        )
        .map_err(|e| format!("Enabling breakpoint site failed: {}", e))?;
        self.is_enabled_ = false;
        Ok(())
    }
    fn enable(&mut self, process: &Process) -> Result<(), String> {
        if self.is_enabled_ {
            return Ok(());
        }
        let data = ptrace::read(
            process.pid(),
            self.virtual_address.addr() as usize as *mut _, // the underscore lets the compiler
                                                            // dynamically infer the type
        )
        .map_err(|e| {
            format!(
                "Couldnt read data at {}, {}",
                self.virtual_address.addr(),
                e
            )
        })?;

        self.instruction_replaced = (data & 0xff) as u8; //extract the lowest 8 bits as 0xff ==
                                                         //11111111

        // Prepare new data with int3 (0xcc) in the lowest byte
        let int3: i64 = 0xcc;
        let data_with_int3 = (data & !0xff) | int3;

        // Write back the modified data with int3
        ptrace::write(
            process.pid(),
            self.virtual_address.addr() as usize as *mut _,
            data_with_int3,
        )
        .map_err(|e| format!("Enabling breakpoint site failed: {}", e))?;

        self.is_enabled_ = true;
        Ok(())
    }
}
