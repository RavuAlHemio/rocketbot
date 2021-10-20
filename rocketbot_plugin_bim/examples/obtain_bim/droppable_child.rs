use std::process::Child;


pub(crate) struct DroppableChild {
    pub child: Child,
}
impl From<Child> for DroppableChild {
    fn from(child: Child) -> Self {
        Self {
            child,
        }
    }
}
impl Drop for DroppableChild {
    fn drop(&mut self) {
        match self.child.kill() {
            // we don't care either way
            Ok(_) => {},
            Err(_) => {},
        }
    }
}
