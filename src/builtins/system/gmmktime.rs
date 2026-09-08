//! Purpose:
//! Home of the PHP `gmmktime` builtin: its declaration and semantic metadata.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - Optional civil fields use a single post-argument UTC clock snapshot.


builtin! {
    contract: "gmmktime",
    semantics: super::mktime::mktime_semantics(true),
}
