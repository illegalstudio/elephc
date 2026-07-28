//! Purpose:
//! Declares the eval-visible `escapeshellcmd` builtin.
//!
//! Called from:
//! - The registry's shared Slashes hook.
//!
//! Key details:
//! - The leaf delegates platform-specific escaping to `shell_escape`.

eval_builtin! {
    name: "escapeshellcmd",
    area: String,
    params: [command],
    direct: Slashes,
    values: Slashes,
}
