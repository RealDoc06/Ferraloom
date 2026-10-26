//! CommandFunction compilation and the server's canonical function library.
mod library;
mod plain;
mod runtime;
pub use library::{FunctionLibrary, FunctionLoadError};
pub use plain::{CompiledFunction, FunctionParseError, PlainTextFunction, read_lines};
pub use runtime::queue_function;
#[cfg(test)]
mod tests;
