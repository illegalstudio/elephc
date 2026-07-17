//! Purpose:
//! Registry direct-hook invariant tests for source-sensitive builtins.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Assertions use registry metadata APIs rather than dispatcher literals.

use super::*;

/// Verifies direct-call fallback is needed only for source-sensitive pre-dispatched builtins.
    #[test]
    fn declared_builtin_registry_marks_only_pre_dispatched_builtins_without_direct_hooks() {
        let mut without_direct: Vec<&str> = eval_declared_builtin_function_names()
            .iter()
            .copied()
            .filter(|name| {
                eval_declared_builtin_spec(name)
                    .is_some_and(|spec| spec.direct.is_none())
            })
            .collect();
        without_direct.sort_unstable();

        assert_eq!(
            without_direct,
            [
                "array_pop",
                "array_push",
                "array_shift",
                "array_splice",
                "array_unshift",
                "array_walk",
                "arsort",
                "asort",
                "flock",
                "fsockopen",
                "krsort",
                "ksort",
                "natcasesort",
                "natsort",
                "pfsockopen",
                "rsort",
                "settype",
                "shuffle",
                "sort",
                "stream_select",
                "stream_socket_accept",
                "stream_socket_recvfrom",
                "uasort",
                "uksort",
                "usort",
            ]
        );
    }
