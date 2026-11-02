//! Purpose:
//! Joins Magician to the shared runtime builtin contract for `headers_list`.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry` inventory assembly.
//!
//! Key details:
//! - The pending response headers live in the web bridge the compiled `header()` writes, so an
//!   interpreted caller reads them through `RuntimeBuiltinId::HeadersList`; an eval-local list
//!   would miss every header the compiled program set. No Magician algorithm lives here.

eval_builtin! {
    contract: "headers_list",
    area: Time,
    direct: none,
    values: none,
}
