//! Purpose:
//! Binds mb_parse_str to shared query parsing with persistent eval output references.
//!
//! Called from:
//! - Magician's builtin registry and shared runtime dispatcher.
//!
//! Key details:
//! - Shared argument evaluation retains the result reference before dispatching to the V5 host.
//! - Parsing, encoding detection, and live INI configuration stay in the common engine.

eval_builtin! {
    contract: "mb_parse_str",
    area: String,
    direct: none,
    values: none,
}
