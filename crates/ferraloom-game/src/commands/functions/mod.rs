//! CommandFunction compilation and the server's canonical function library.
mod library;
mod manager;
mod plain;
mod runtime;
pub use library::{FunctionLibrary, FunctionLoadError};
pub use manager::{CollectorId, FunctionScope, ServerFunctionManager};
pub use plain::{CompiledFunction, FunctionParseError, PlainTextFunction, read_lines};
pub use runtime::queue_function;
#[cfg(test)]
mod tests;
