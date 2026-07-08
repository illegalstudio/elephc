//! Purpose:
//! Declarative eval registry entry for `stream_context_set_option`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - The signature keeps the existing two-argument and four-argument forms.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_context_set_option",
    area: Filesystem,
    params: [
        context,
        wrapper_or_options,
        option_name = EvalBuiltinDefaultValue::Null,
        value = EvalBuiltinDefaultValue::Null
    ],
    required: 2,
    direct: Filesystem,
    values: Filesystem,
}
