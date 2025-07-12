#[derive(Default, Debug, Clone, Copy)]
pub enum ItemType {
    Watermelon = 45,
    Box = 37,
    BigBox = 38,
    #[default]
    Unknown = 0
}