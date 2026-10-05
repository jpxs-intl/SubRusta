use glam::{Vec3, UVec3};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Vector(pub Vec3);

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct IntVector(pub UVec3);


#[cfg(feature = "binrw")]
mod binrw_impls {
    use super::*;
    use binrw::BinRead;

    impl BinRead for Vector {
        type Args<'a> = ();

        fn read_options<R: std::io::prelude::Read + std::io::prelude::Seek>(
            reader: &mut R,
            endian: binrw::Endian,
            _args: Self::Args<'_>,
        ) -> binrw::prelude::BinResult<Self> {
            Ok(Vector(Vec3::from(<[f32; 3]>::read_options(reader, endian, ())?)))
        }
    }

    impl BinRead for IntVector {
        type Args<'a> = ();
    
        fn read_options<R: std::io::prelude::Read + std::io::prelude::Seek>(
            reader: &mut R,
            endian: binrw::Endian,
            _args: Self::Args<'_>,
        ) -> binrw::prelude::BinResult<Self> {
            Ok(IntVector(UVec3::from(<[u32; 3]>::read_options(reader, endian, ())?)))
        }
    }
}