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
