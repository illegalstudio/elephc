---
type: "Gotcha"
title: "Native static-method dispatch recurses infinitely and aborts the whole lib test process"
description: "Four eval interpreter tests abort the entire cargo test p elephc magician lib PROCESS with a stack overflow, not a failure: dynamic calls::runtime callables::execute program static call dispatches runtime method hook , d"
resource: "crates/elephc-magician/src/interpreter/tests/dynamic_calls/runtime_callables.rs"
tags: ["session-learning", "stack-overflow", "static-dispatch", "test-suite", "eval-interpreter"]
timestamp: "2026-09-06T06:03:09.119Z"
x-kage-id: "repo:lazy-petting-popcorn:gotcha:native-static-method-dispatch-recurses-infinitely-and-aborts-the-whole-lib-test-"
x-kage-type: "gotcha"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["crates/elephc-magician/src/interpreter/tests/dynamic_calls/runtime_callables.rs", "crates/elephc-magician/src/interpreter/tests/method_arguments.rs", "crates/elephc-magician/src/interpreter/statements/native_method_execution.rs"]
x-kage-stack: ["rust", "elephc-magician"]
---

# Native static-method dispatch recurses infinitely and aborts the whole lib test process

> Four eval interpreter tests abort the entire cargo test p elephc magician lib PROCESS with a stack overflow, not a fa…

Four eval interpreter tests abort the entire `cargo test -p elephc-magician --lib` PROCESS with a stack overflow, not a failure: `dynamic_calls::runtime_callables::execute_program_static_call_dispatches_runtime_method_hook`, `dynamic_calls::runtime_callables::execute_program_static_runtime_method_hook_binds_default_args`, `dynamic_calls::runtime_callables::execute_program_static_runtime_method_hook_rejects_by_ref_temporary_arg`, and `method_arguments::runtime_fallback::execute_program_writes_back_runtime_static_method_by_ref_before_fatal`. All four go through native or AOT STATIC method dispatch with a registered native signature.

It is infinite recursion, not a stack-size problem: raising RUST_MIN_STACK to 256 MB still overflows. Because the abort kills the process, no full lib run completes and every before/after sweep is silently truncated at whichever of the four runs first. Sweeps must pass `-- --skip runtime_callables --skip runtime_fallback` to get a comparable number at all.

Confirmed pre-existing rather than introduced: reverting the working tree to the base reproduces it, and no-op'ing newly added call-frame pushes does not change it.
Evidence: `return KnownClass::sum(1, 2);` with a native static method signature whose first parameter is by-reference overflows the stack at RUST_MIN_STACK=33554432 and again at 268435456. The remaining ten tests in the same two modules pass individually.
Verified by: Ran each of the fourteen tests individually; four abort, ten pass. Reproduced at the base commit with the working tree reverted.

## Verification

`return KnownClass::sum(1, 2);` with a native static method signature whose first parameter is by-reference overflows the stack at RUST_MIN_STACK=33554432 and again at 268435456. The remaining ten tests in the same two modules pass individually.

# Citations

[1] explicit_capture (2026-09-06T06:03:09.119Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:gotcha:native-static-method-dispatch-recurses-infinitely-and-aborts-the-whole-lib-test-","title":"Native static-method dispatch recurses infinitely and aborts the whole lib test process","summary":"Four eval interpreter tests abort the entire cargo test p elephc magician lib PROCESS with a stack overflow, not a failure: dynamic calls::runtime callables::execute program static call dispatches runtime method hook , d","body":"Four eval interpreter tests abort the entire `cargo test -p elephc-magician --lib` PROCESS with a stack overflow, not a failure: `dynamic_calls::runtime_callables::execute_program_static_call_dispatches_runtime_method_hook`, `dynamic_calls::runtime_callables::execute_program_static_runtime_method_hook_binds_default_args`, `dynamic_calls::runtime_callables::execute_program_static_runtime_method_hook_rejects_by_ref_temporary_arg`, and `method_arguments::runtime_fallback::execute_program_writes_back_runtime_static_method_by_ref_before_fatal`. All four go through native or AOT STATIC method dispatch with a registered native signature.\n\nIt is infinite recursion, not a stack-size problem: raising RUST_MIN_STACK to 256 MB still overflows. Because the abort kills the process, no full lib run completes and every before/after sweep is silently truncated at whichever of the four runs first. Sweeps must pass `-- --skip runtime_callables --skip runtime_fallback` to get a comparable number at all.\n\nConfirmed pre-existing rather than introduced: reverting the working tree to the base reproduces it, and no-op'ing newly added call-frame pushes does not change it.\nEvidence: `return KnownClass::sum(1, 2);` with a native static method signature whose first parameter is by-reference overflows the stack at RUST_MIN_STACK=33554432 and again at 268435456. The remaining ten tests in the same two modules pass individually.\nVerified by: Ran each of the fourteen tests individually; four abort, ten pass. Reproduced at the base commit with the working tree reverted.","type":"gotcha","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","stack-overflow","static-dispatch","test-suite","eval-interpreter"],"paths":["crates/elephc-magician/src/interpreter/tests/dynamic_calls/runtime_callables.rs","crates/elephc-magician/src/interpreter/tests/method_arguments.rs","crates/elephc-magician/src/interpreter/statements/native_method_execution.rs"],"stack":["rust","elephc-magician"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-06T06:03:09.119Z"}],"context":{"fact":"Four eval interpreter tests abort the entire `cargo test -p elephc-magician --lib` PROCESS with a stack overflow, not a failure: `dynamic_calls::runtime_callables::execute_program_static_call_dispatches_runtime_method_hook`, `dynamic_calls::runtime_callables::execute_program_static_runtime_method_hook_binds_default_args`, `dynamic_calls::runtime_callables::execute_program_static_runtime_method_hook_rejects_by_ref_temporary_arg`, and `method_arguments::runtime_fallback::execute_program_writes_back_runtime_static_method_by_ref_before_fatal`. All four go through native or AOT STATIC method dispatch with a registered native signature.","verification":"`return KnownClass::sum(1, 2);` with a native static method signature whose first parameter is by-reference overflows the stack at RUST_MIN_STACK=33554432 and again at 268435456. The remaining ten tests in the same two modules pass individually."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-06T06:03:09.119Z","path_fingerprints":[{"path":"crates/elephc-magician/src/interpreter/tests/dynamic_calls/runtime_callables.rs","sha256":"c033b16d3a426c377f0555f2ec62cb9e162624c69f4a7a31b4acd2458a9edbe2","size":9228},{"path":"crates/elephc-magician/src/interpreter/statements/native_method_execution.rs","sha256":"e1fcfa4d2c3291090becc31eb289277ad8132371111fd2d7fd4073d49c1de44a","size":21591}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":8000,"discovery_tokens_estimated":true,"score":82,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","concise but substantive","actionable rationale or verification"],"risks":["some referenced paths are missing: crates/elephc-magician/src/interpreter/tests/method_arguments.rs"],"duplicate_candidates":[],"stale_reasons":["some referenced paths are missing: crates/elephc-magician/src/interpreter/tests/method_arguments.rs"],"estimated_tokens_saved":391},"created_at":"2026-09-06T06:03:09.119Z","updated_at":"2026-09-06T06:03:09.119Z","author_branch":"reconcile/dirname-symfony"}
```

