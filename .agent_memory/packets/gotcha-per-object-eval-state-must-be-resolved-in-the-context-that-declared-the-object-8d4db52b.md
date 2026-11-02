---
type: "Gotcha"
title: "Per-object eval state must be resolved in the context that DECLARED the object"
description: "Every per object table on ElephcEvalContext dynamic property values , dynamic initialized properties , dynamic property order , dynamic objects is keyed by identity but stored ONLY in the context that created the object."
resource: "crates/elephc-magician/src/context/classlike_objects.rs"
tags: ["session-learning", "eval-bridge", "contexts", "symfony", "unserialize", "serialize"]
timestamp: "2026-09-13T08:30:54.022Z"
x-kage-id: "repo:lazy-petting-popcorn:gotcha:per-object-eval-state-must-be-resolved-in-the-context-that-declared-the-object-1"
x-kage-type: "gotcha"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["crates/elephc-magician/src/context/classlike_objects.rs", "crates/elephc-magician/src/interpreter/builtins/core/print_r.rs", "crates/elephc-magician/src/ffi/context.rs"]
---

# Per-object eval state must be resolved in the context that DECLARED the object

> Every per object table on ElephcEvalContext dynamic property values , dynamic initialized properties , dynamic proper…

Every per-object table on `ElephcEvalContext` (`dynamic_property_values`, `dynamic_initialized_properties`, `dynamic_property_order`, `dynamic_objects`) is keyed by identity but stored ONLY in the context that created the object. A second context asking about the same object must fall back to the owner, which `crate::ffi::dynamic_destructors::dynamic_object_owner_context(identity)` resolves. Three distinct bugs in one Symfony request traced to this:

1. `dynamic_property_value` / `dynamic_property_is_initialized` / `dynamic_property_storage_names` had no owner fallback, so an object hydrated by the `unserialize` FFI hook (which runs in `shared_null_handle_context()`) arrived in the request's context with its class intact and every slot empty -- reported as `Typed property X::$p must not be accessed before initialization`. Fixed by adding `dynamic_object_foreign_owner()` in `crates/elephc-magician/src/context/classlike_objects.rs` and routing those three accessors through it.

2. `context.dynamic_object_class(identity)` resolves the OWNER's class NAME against the ASKER's class table, so it answers `None` whenever the asker has not itself declared a class of that name. `context.dynamic_object_declaring_class(identity)` returns `(owner_context, class)` and is the correct call. `eval_debug_object_class_name` / `eval_debug_object_properties` (shared by `print_r`, `var_dump`, `serialize`, `var_export`) used the former, so a cross-context `serialize()` wrote `O:8:"stdClass":0:{}`.

3. The class ancestry for an object's declared properties must come from the owner too: `context.class_chain(class_name)` is empty in a context that never declared the class. `eval_debug_object_class_chain()` now picks the declaring context.

The general rule: if you are about to ask a context something about an OBJECT IDENTITY, ask whether the object could have been built elsewhere. On a Symfony request it always can be -- the compiled DI container lives behind a computed `require`, so every class it names is eval-declared and every included file gets its own context.
Evidence: Reproduced with a 15-line probe (eval-declared class, unserialized from AOT, read back inside a second eval) that failed with the exact Symfony error before the fix and matches `php -n` 8.5.10 after. Regression test: tests/codegen/serialize.rs::test_unserialize_eval_declared_object_reads_back_in_another_context.
Verified by: cargo test --release --test codegen_tests -- serialize::test_unserialize_eval_declared_object

## Verification

Reproduced with a 15-line probe (eval-declared class, unserialized from AOT, read back inside a second eval) that failed with the exact Symfony error before the fix and matches `php -n` 8.5.10 after. Regression test: tests/codegen/serialize.rs::test_unserialize_eval_declared_object_reads_back_in_another_context.

# Citations

[1] explicit_capture (2026-09-13T08:30:54.022Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:gotcha:per-object-eval-state-must-be-resolved-in-the-context-that-declared-the-object-1","title":"Per-object eval state must be resolved in the context that DECLARED the object","summary":"Every per object table on ElephcEvalContext dynamic property values , dynamic initialized properties , dynamic property order , dynamic objects is keyed by identity but stored ONLY in the context that created the object.","body":"Every per-object table on `ElephcEvalContext` (`dynamic_property_values`, `dynamic_initialized_properties`, `dynamic_property_order`, `dynamic_objects`) is keyed by identity but stored ONLY in the context that created the object. A second context asking about the same object must fall back to the owner, which `crate::ffi::dynamic_destructors::dynamic_object_owner_context(identity)` resolves. Three distinct bugs in one Symfony request traced to this:\n\n1. `dynamic_property_value` / `dynamic_property_is_initialized` / `dynamic_property_storage_names` had no owner fallback, so an object hydrated by the `unserialize` FFI hook (which runs in `shared_null_handle_context()`) arrived in the request's context with its class intact and every slot empty -- reported as `Typed property X::$p must not be accessed before initialization`. Fixed by adding `dynamic_object_foreign_owner()` in `crates/elephc-magician/src/context/classlike_objects.rs` and routing those three accessors through it.\n\n2. `context.dynamic_object_class(identity)` resolves the OWNER's class NAME against the ASKER's class table, so it answers `None` whenever the asker has not itself declared a class of that name. `context.dynamic_object_declaring_class(identity)` returns `(owner_context, class)` and is the correct call. `eval_debug_object_class_name` / `eval_debug_object_properties` (shared by `print_r`, `var_dump`, `serialize`, `var_export`) used the former, so a cross-context `serialize()` wrote `O:8:\"stdClass\":0:{}`.\n\n3. The class ancestry for an object's declared properties must come from the owner too: `context.class_chain(class_name)` is empty in a context that never declared the class. `eval_debug_object_class_chain()` now picks the declaring context.\n\nThe general rule: if you are about to ask a context something about an OBJECT IDENTITY, ask whether the object could have been built elsewhere. On a Symfony request it always can be -- the compiled DI container lives behind a computed `require`, so every class it names is eval-declared and every included file gets its own context.\nEvidence: Reproduced with a 15-line probe (eval-declared class, unserialized from AOT, read back inside a second eval) that failed with the exact Symfony error before the fix and matches `php -n` 8.5.10 after. Regression test: tests/codegen/serialize.rs::test_unserialize_eval_declared_object_reads_back_in_another_context.\nVerified by: cargo test --release --test codegen_tests -- serialize::test_unserialize_eval_declared_object","type":"gotcha","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","eval-bridge","contexts","symfony","unserialize","serialize"],"paths":["crates/elephc-magician/src/context/classlike_objects.rs","crates/elephc-magician/src/interpreter/builtins/core/print_r.rs","crates/elephc-magician/src/ffi/context.rs"],"stack":[],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-13T08:30:54.022Z"}],"context":{"fact":"Every per-object table on `ElephcEvalContext` (`dynamic_property_values`, `dynamic_initialized_properties`, `dynamic_property_order`, `dynamic_objects`) is keyed by identity but stored ONLY in the context that created the object. A second context asking about the same object must fall back to the owner, which `crate::ffi::dynamic_destructors::dynamic_object_owner_context(identity)` resolves. Three distinct bugs in one Symfony request traced to this:","verification":"Reproduced with a 15-line probe (eval-declared class, unserialized from AOT, read back inside a second eval) that failed with the exact Symfony error before the fix and matches `php -n` 8.5.10 after. Regression test: tests/codegen/serialize.rs::test_unserialize_eval_declared_object_reads_back_in_another_context."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-13T08:30:54.022Z","path_fingerprints":[{"path":"crates/elephc-magician/src/context/classlike_objects.rs","sha256":"07ddbc27cdd30f01e60d99c67a3cba95bd54cd78190476d8e4e55379c5472e94","size":31011},{"path":"crates/elephc-magician/src/interpreter/builtins/core/print_r.rs","sha256":"9d51904d3970014b47ae5249c61a9e5c99060203c551a50eb0ecc7dd5d83e965","size":16468},{"path":"crates/elephc-magician/src/ffi/context.rs","sha256":"0862d9f7cfcb2687440a47d3f90d43d47dfba337d907116aef006d8517911f2d","size":27199}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":8000,"discovery_tokens_estimated":true,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"stale_reasons":[],"estimated_tokens_saved":627},"created_at":"2026-09-13T08:30:54.022Z","updated_at":"2026-09-13T08:30:54.022Z","author_branch":"reconcile/dirname-symfony"}
```

