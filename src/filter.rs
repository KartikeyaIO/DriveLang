use crate::media::frame::{Frame, PixelData};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct Filter {
    pub name: String,
    pub ops: Ops,
}
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct Ops {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}
