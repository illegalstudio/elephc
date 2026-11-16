//! Purpose:
//! Declares the Magician binding for `unserialize`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Execution uses the shared boxed-cell runtime parser with the default
//!   allowed-classes policy. The optional `$options` argument is not registered
//!   yet; `eval()` currently supports the one-argument form.

eval_builtin! {
    contract: "unserialize",
    area: Core,
    direct: none,
    values: none,
}
