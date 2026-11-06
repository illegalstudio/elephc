//! Purpose:
//! Declarative eval registry entry for `ctype_cntrl`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - The shared Ctype hook implements the predicate, the deprecated scalar fallback,
//!   and the runtime-type deprecation in `ctype_alnum.rs`.

eval_builtin! {
    contract: "ctype_cntrl",
    area: String,
    direct: Ctype,
    values: Ctype,
}
