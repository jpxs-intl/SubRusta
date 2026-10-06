pub mod sbl;
pub mod srk;

#[derive(Debug)]
pub enum LoaderError {
    Io(std::io::Error),
    Parse(binrw::Error)
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
        }
    }
}
impl std::error::Error for LoaderError {}