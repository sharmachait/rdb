use crate::rdb::virtual_address::VirtAddr;

// Trait that stoppoint types must implement
pub trait Stoppoint {
    fn id(&self) -> usize;
    fn address(&self) -> &VirtAddr;
    fn is_enabled(&self) -> bool;
}

pub struct StoppointCollection<S: Stoppoint> {
    stoppoints: Vec<S>,
}

impl <S: Stoppoint> StoppointCollection<S> {
    pub fn push(&mut self, sp: S) -> &mut S {
        self.stoppoints.push(sp);
        self.stoppoints.last_mut().unwrap()
    }
    pub fn contains_id(&self, id: usize) -> bool {
        self.stoppoints.iter().any(|sp| sp.id() == id)
    }
    pub fn contains_address(&self, addr: &VirtAddr) -> bool {
        self.stoppoints.iter().any(|sp| sp.address() == addr)
    }
    pub fn is_stoppoint_enabled_by_address(&self, addr: &VirtAddr) -> bool {
        self.stoppoints
            .iter()
            .find(|sp| sp.address() == addr)
            .map(|sp| sp.is_enabled())
            .unwrap_or(false)
    }
    pub fn get_by_id(&self, id: usize) -> Option<&S> {
        self.stoppoints.iter()
            .find(|sp| sp.id() == id)
    }
    pub fn get_by_address(&self, virt_addr: &VirtAddr) -> Option<&S> {
        self.stoppoints.iter()
            .find(|sp| sp.address() == virt_addr)
    }
    pub fn get_by_id_mut(&mut self, id: usize) -> Option<&mut S> {
        self.stoppoints.iter_mut()
            .find(|sp| sp.id() == id)
    }
    pub fn get_by_address_mut(&mut self, virt_addr: &VirtAddr) -> Option<&mut S> {
        self.stoppoints.iter_mut()
            .find(|sp| sp.address() == virt_addr)
    }
    pub fn remove_by_id(&mut self, id: usize) -> Option<S> {
        if let Some(pos) = self.stoppoints.iter().position(|sp| sp.id() == id) {
            Some(self.stoppoints.remove(pos))
        } else {
            None
        }
    }
    pub fn remove_by_address(&mut self, addr: &VirtAddr) -> Option<S> {
        if let Some(pos) = self.stoppoints.iter().position(|sp| sp.address() == addr) {
            Some(self.stoppoints.remove(pos))
        } else {
            None
        }
    }
    pub fn is_empty(&self) -> bool {
        self.stoppoints.is_empty()
    }
    pub fn len(&self) -> usize {
        self.stoppoints.len()
    }
    pub fn for_each(&self, f: impl Fn(&S)) {
        for sp in &self.stoppoints {
            f(sp);
        }
    }
    pub fn for_each_mut(&mut self, mut f: impl FnMut(&mut S)) { // here f is defined as mut because a mutable closure needs to be mutable as well
        for sp in &mut self.stoppoints {
            f(sp);
        }
    }
}

impl<S: Stoppoint> Default for StoppointCollection<S> {
    fn default() -> Self {
        Self { stoppoints: Vec::new() }
    }
}