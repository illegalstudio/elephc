//! Purpose:
//! Declares the eval-visible `escapeshellarg` builtin.
//!
//! Called from:
//! - The registry's shared Slashes hook.
//!
//! Key details:
//! - The leaf delegates platform-specific escaping to `shell_escape`.

eval_builtin! {
    contract: "escapeshellarg",
    area: String,
    direct: Slashes,
    values: Slashes,
}
