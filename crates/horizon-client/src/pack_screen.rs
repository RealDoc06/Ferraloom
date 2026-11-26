/// `PackCompatibility`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compat {
    Compatible,
    TooOld,
    TooNew,
    Unknown,
}
