//! Purpose:
//! Declares the eval-visible `escapeshellcmd` builtin.
//!
//! Called from:
//! - The registry's shared Slashes hook.
//!
//! Key details:
//! - The leaf delegates platform-specific escaping to `shell_escape`.

eval_builtin! {
    contract: "escapeshellcmd",
    area: String,
    direct: Slashes,
    values: Slashes,
}
