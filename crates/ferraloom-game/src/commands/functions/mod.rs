//! CommandFunction compilation and the server's canonical function library.
mod library;
mod plain;
pub use library::{FunctionLibrary, FunctionLoadError};
pub use plain::{CompiledFunction, FunctionParseError, PlainTextFunction, read_lines};
#[cfg(test)]
mod tests;
