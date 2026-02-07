#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub struct VirtAddr {
    addr_: u64,
}

impl VirtAddr {
    pub fn new() -> Self {
        Self { addr_: 0 }
    }

    pub fn with_addr(addr: u64) -> Self {
        Self { addr_: addr }
    }

    pub fn addr(&self) -> u64 {
        self.addr_
    }
}

impl Default for VirtAddr {
    fn default() -> Self {
        Self::new()
    }
}
//+
impl std::ops::Add<i64> for VirtAddr {
    type Output = VirtAddr;
    fn add(self, offset: i64) -> VirtAddr {
        VirtAddr::with_addr(self.addr_.wrapping_add_signed(offset))
    }
}
//-
impl std::ops::Sub<i64> for VirtAddr {
    type Output = VirtAddr;
    fn sub(self, offset: i64) -> VirtAddr {
        VirtAddr::with_addr(self.addr_.wrapping_sub_signed(offset))
    }
}

//+=
impl std::ops::AddAssign<i64> for VirtAddr {
    fn add_assign(&mut self, offset: i64) {
        self.addr_ = self.addr_.wrapping_add_signed(offset);
    }
}
//-=
impl std::ops::SubAssign<i64> for VirtAddr {
    fn sub_assign(&mut self, offset: i64) {
        self.addr_ = self.addr_.wrapping_sub_signed(offset);
    }
}
// ==
// !=
// a == a
// > < >= <=
