//! Purpose:
//! Declarative eval registry entry for `stream_select`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Direct calls keep their source-sensitive by-reference array path.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_select",
    area: Filesystem,
    params: [
        read: by_ref,
        write: by_ref,
        except: by_ref,
        seconds,
        microseconds = EvalBuiltinDefaultValue::Int(0)
    ],
    by_ref: [read, write, except],
    direct: none,
    values: Filesystem,
}
