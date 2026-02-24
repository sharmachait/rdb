use nix::{sys::ptrace, unistd::Pid};

use crate::rdb::{
    process::Process, stop_points::stoppoint_collection::Stoppoint, virtual_address::VirtAddr,
};

pub struct BreakpointVA {
    id: usize,
    virtual_address: VirtAddr,
    is_enabled_: bool,
    instruction_replaced: u8,
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
    fn enable(&mut self, pid: Pid) -> Result<(), String> {
        if self.is_enabled_ {
            return Ok(());
        }
        let data =
            ptrace::read(pid, self.virtual_address.addr() as usize as *mut _).map_err(|e| {
                format!(
                    "Couldnt read data at {}, {}",
                    self.virtual_address.addr(),
                    e
                )
            })?;
        self.instruction_replaced = (data & 0xff) as u8; //0xff == 11111111
        let int3: i64 = 0xcc;
        let data_with_int3 = (data & !0xff) | int3;
        ptrace::write(
            pid,
            self.virtual_address.addr() as usize as *mut _,
            data_with_int3,
        )
        .map_err(|e| {
            format!(
                "Couldnt write data to {}, {}",
                self.virtual_address.addr(),
                e
            )
        });
        self.is_enabled_ = true;
        Ok(())
    }
    fn disable(&mut self, pid: Pid) -> Result<(), String> {
        if !self.is_enabled_ {
            return Ok(());
        }
        let data =
            ptrace::read(pid, self.virtual_address.addr() as usize as *mut _).map_err(|e| {
                format!(
                    "Couldnt read data at {}, {}",
                    self.virtual_address.addr(),
                    e
                )
            })?;
        let data_restored = (data & !0xff) | (self.instruction_replaced as i64);
        ptrace::write(
            pid,
            self.virtual_address.addr() as usize as *mut _,
            data_restored,
        )
        .map_err(|e| {
            format!(
                "Couldnt write data to {}, {}",
                self.virtual_address.addr(),
                e
            )
        });
        self.is_enabled_ = false;
        Ok(())
    }
}

impl BreakpointVA {
    pub fn create_for_process_at(process: &mut Process, addr: VirtAddr) -> BreakpointVA {
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
}
