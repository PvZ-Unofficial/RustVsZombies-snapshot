#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub(crate) struct RawRect {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: i32,
    pub(crate) height: i32,
}

impl RawRect {
    pub(crate) const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self { x, y, width, height }
    }
}
