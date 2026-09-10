//! Purpose:
//! Registry direct-hook invariant tests for source-sensitive builtins.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Assertions use registry metadata APIs rather than dispatcher literals.

use super::*;

/// Verifies non-runtime direct-call fallback is limited to source-sensitive pre-dispatch.
#[test]
fn declared_builtin_registry_marks_only_pre_dispatched_adapters_without_direct_hooks() {
        let mut without_direct: Vec<&str> = eval_declared_builtin_function_names()
            .iter()
            .copied()
            .filter(|name| {
                eval_declared_builtin_spec(name)
                    .is_some_and(|spec| spec.direct.is_none() && spec.runtime_builtin.is_none())
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
                "end",
                "flock",
                "fsockopen",
                "krsort",
                "ksort",
                "natcasesort",
                "natsort",
                "next",
                "pfsockopen",
                "prev",
                "reset",
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

/// Verifies implemented runtime bindings use shared dispatch and unsupported contracts stay absent.
#[test]
fn runtime_builtin_bindings_keep_only_intval_and_round_adapters() {
    for runtime_id in elephc_builtin_contract::RuntimeBuiltinId::ALL {
        let contract = elephc_builtin_contract::lookup_id(runtime_id.builtin_id())
            .expect("runtime builtin contract must exist");
        if !matches!(elephc_builtin_contract::eval_support(contract),
            elephc_builtin_contract::BackendSupport::Implemented(_)) {
            assert!(eval_raw_declared_builtin_spec(contract.name).is_none(),
                "{} must remain unavailable until its declared adapter is implemented", contract.name);
            continue;
        }
        let spec = eval_raw_declared_builtin_spec(contract.name)
            .expect("runtime builtin must have an eval binding");
        assert_eq!(spec.runtime_builtin, Some(runtime_id));
        if matches!(
            runtime_id,
            elephc_builtin_contract::RuntimeBuiltinId::Intval
                | elephc_builtin_contract::RuntimeBuiltinId::Round
        ) {
            assert!(spec.direct.is_some(), "{} needs a direct adapter", spec.name);
            assert!(spec.values.is_some(), "{} needs a values adapter", spec.name);
        } else {
            assert!(spec.direct.is_none(), "{} must use runtime dispatch", spec.name);
            assert!(spec.values.is_none(), "{} must use runtime dispatch", spec.name);
        }
    }
}
