pub mod sbl;
pub mod srk;
pub mod sbc;
pub mod csx;
pub mod sbb;
pub mod cmo;
pub mod sbv;
pub mod tst;

#[derive(Debug)]
pub enum LoaderError {
    Io(std::io::Error),
    Parse(binrw::Error),
    NotFound
}

impl From<std::io::Error> for LoaderError {
    fn from(e: std::io::Error) -> Self { LoaderError::Io(e) }
}
impl From<binrw::Error> for LoaderError {
    fn from(e: binrw::Error) -> Self { LoaderError::Parse(e) }
}

impl std::fmt::Display for LoaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoaderError::Io(e)    => write!(f, "map io error: {e}"),
            LoaderError::Parse(e) => write!(f, "map parse error: {e}"),
            LoaderError::NotFound => write!(f, "map not found error")
        }
    }
}
impl std::error::Error for LoaderError {}