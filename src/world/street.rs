use binrw::{BinRead, BinWrite};

#[derive(BinRead, BinWrite, Clone)]
pub struct FileStreet {
    pub intersection_indices: [u32; 2],
    pub direction: u32,
    pub left_lane: u32,
    pub right_lane: u32,
    #[br(count = 32)]
    pub name: Vec<u8>
}