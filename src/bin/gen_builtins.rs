//! Purpose:
//! Standalone tool that prints the single-source PHP builtin registry as documentation JSON.
//!
//! Called from:
//! - `cargo run --bin gen_builtins` (documentation generation / CI docs export).
//!
//! Key details:
//! - Delegates all logic to `elephc::builtins::docs`; this binary only serializes the value to
//!   pretty JSON on stdout.
//! - `--include-internal` also emits `internal: true` builtins (the docs pipeline renders
//!   compiler-internals pages for the `__elephc_*` helpers). Without it, only the PHP-visible
//!   surface is emitted.

/// Prints the builtin documentation JSON (pretty-printed) to stdout.
///
/// Emits the PHP-visible builtin surface by default; pass `--include-internal` to also include
/// `internal` builtins (used by the documentation generator).
fn main() {
    let include_internal = std::env::args().any(|a| a == "--include-internal");
    let value = if include_internal {
        elephc::builtins::docs::export_builtins_json_all()
    } else {
        elephc::builtins::docs::export_builtins_json()
    };
    let json = serde_json::to_string_pretty(&value).expect("serialize builtins JSON");
    println!("{}", json);
}
