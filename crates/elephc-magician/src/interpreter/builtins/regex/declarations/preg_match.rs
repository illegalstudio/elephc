//! Purpose:
//! Declarative eval registry entry for `preg_match`.
//!
//! Called from:
//! - `crate::interpreter::builtins::regex::declarations`.
//!
//! Key details:
//! - `$matches` is by-reference and is still written through the regex direct
//!   and dynamic mutation helpers.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "preg_match",
    area: Regex,
    params: [
        pattern,
        subject,
        matches: by_ref = EvalBuiltinDefaultValue::EmptyArray,
        flags = EvalBuiltinDefaultValue::Int(0),
    ],
    by_ref: [matches],
    direct: Regex,
    values: Regex,
}
