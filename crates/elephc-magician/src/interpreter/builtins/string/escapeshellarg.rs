//! Purpose:
//! Declares the eval-visible `escapeshellarg` builtin.
//!
//! Called from:
//! - The registry's shared Slashes hook.
//!
//! Key details:
//! - The leaf delegates platform-specific escaping to `shell_escape`.

eval_builtin! {
    name: "escapeshellarg",
    area: String,
    params: [arg],
    direct: Slashes,
    values: Slashes,
}
