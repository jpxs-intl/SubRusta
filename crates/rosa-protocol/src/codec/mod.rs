mod reader;
mod writer;

pub use crate::codec::reader::Reader;
pub use crate::codec::writer::Writer;

#[derive(Debug)]
pub enum CodecError { EoF, TooLong, BadEnum(u32), Utf8 }

pub trait WireRead: Sized {
    fn read(r: &mut Reader) -> Result<Self, CodecError>;
}

pub trait WireWrite {
   fn write(&self, w: &mut Writer);
}