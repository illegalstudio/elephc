//! Purpose:
//! Binds eval output conversion to the same request and protected host ABI used by native calls.
//!
//! Called from:
//! - The Magician builtin registry and versioned runtime builtin dispatcher.
//!
//! Key details:
//! - The native runtime supplies response metadata and current output-handler flags.

eval_builtin! {
    contract: "mb_output_handler",
    area: String,
    direct: none,
    values: none,
}
