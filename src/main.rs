//! Purpose:
//! Provides the binary entry point for the elephc compiler.
//! Wires CLI parsing to the ordered compile pipeline without owning compiler logic.
//!
//! Called from:
//! - The operating system when running the `elephc` executable.
//!
//! Key details:
//! - Keep startup thin so CLI validation and pipeline behavior stay in dedicated modules.

mod autoload;
mod cli;
mod codegen;
mod conditional;
mod errors;
mod linker;
mod lexer;
mod magic_constants;
mod name_resolver;
mod names;
mod optimize;
mod parser;
mod pipeline;
mod resolver;
mod runtime_cache;
mod source_map;
mod span;
mod termination;
mod timings;
mod types;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let config = cli::parse_args(&args);
    pipeline::compile(config);
}
