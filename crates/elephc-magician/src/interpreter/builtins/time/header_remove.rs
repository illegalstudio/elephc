//! Purpose:
//! Joins Magician to the shared runtime builtin contract for `header_remove`.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry` inventory assembly.
//!
//! Key details:
//! - Removal acts on the web bridge list the compiled `header()` fills, through
//!   `RuntimeBuiltinId::HeaderRemove`: no argument or `null` clears every pending header, a name
//!   removes the headers of that name case-insensitively. No Magician algorithm lives here.

eval_builtin! {
    contract: "header_remove",
    area: Time,
    direct: none,
    values: none,
}
