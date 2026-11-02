---
type: "Bug Fix"
title: "An `iterable` parameter crossing eval->AOT was handed the BOXED CELL instead of the payload, so a Generator arrived as an array"
description: "FIXED. An AOT function called from interpreted code received the boxed Mixed cell for any parameter declared iterable , where the AOT side expects the unboxed payload. A Generator passed that way arrived as an ARRAY sile"
resource: "src/codegen/runtime_callable_invoker.rs"
tags: ["session-learning", "symfony-web", "iterable", "generator", "eval-bridge", "abi", "codegen"]
timestamp: "2026-09-12T18:30:15.588Z"
x-kage-id: "repo:lazy-petting-popcorn:bug_fix:an-iterable-parameter-crossing-eval-aot-was-handed-the-boxed-cell-instead-of-the"
x-kage-type: "bug_fix"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["src/codegen/runtime_callable_invoker.rs", "src/codegen/eval_method_helpers.rs", "src/ir_lower/gradual_coercions.rs", "src/types/model.rs"]
x-kage-stack: ["rust", "php", "symfony", "macos-aarch64"]
---

# An `iterable` parameter crossing eval->AOT was handed the BOXED CELL instead of the payload, so a Generator arrived as an array

> FIXED. An AOT function called from interpreted code received the boxed Mixed cell for any parameter declared iterable…

FIXED. An AOT function called from interpreted code received the boxed Mixed cell for any parameter declared `iterable`, where the AOT side expects the unboxed payload. A Generator passed that way arrived as an ARRAY -- silently, with no crash and no diagnostic.

THE ONE-LINE SYMPTOM, and it needs no crash to show:
    function what(iterable $v): string { return get_debug_type($v); }
  php -n 8.5.10 : AOT gen=Generator arr=array   EVAL gen=Generator arr=array
  before        : AOT gen=Generator arr=array   EVAL gen=array     arr=array
  after         : identical to php
The array half was always right, which is what made the bug look like a Generator problem rather
than a representation problem.

THE SITE. `coerce_result_to_type` in src/codegen/runtime_callable_invoker.rs: when the source is
`Mixed`, the match unboxes for `Array(_) | AssocArray { .. } | Object(_)` and passes through for
`Mixed | Union(_)`. `PhpType::Iterable` matched NEITHER and fell into the trailing `_ => {}`, so
no `__rt_mixed_unbox` was emitted and the cell pointer itself became the argument. The fix adds
`PhpType::Iterable` to the unboxing arm, and to the allowed-target list in
`can_coerce_result_to_type` so the coercion is actually attempted. This is the same class of
omission as the `instanceof` one: `iterable` keeps being forgotten in matches that enumerate the
container types, because it is spelled like a scalar-ish keyword but behaves like `array|Traversable`.

WHY THE ABI IS THIS WAY: coercing a gradual value TO `iterable` emits `Op::MixedUnbox`
(src/ir_lower/gradual_coercions.rs:104) and `src/types/model.rs:120` describes the slot as a
"type-erased pointer (array|Traversable)". The METHOD and CONSTRUCTOR bridges already got this
right through a dedicated `emit_*_cast_eval_iterable_arg` (src/codegen/eval_method_helpers.rs)
that unboxes, accepts tags 4/5/6 and validates an object against the Traversable interface ids;
only the callable-invoker path -- which is how an AOT FUNCTION is exposed to the interpreter via
`register_eval_native_function` -- was missing it.

WHAT IT DOES NOT FIX, measured immediately after. The value now arrives correctly, but DRIVING an
interpreter-created Generator from AOT code still fails: the same probe extended to call
`valid()` / `current()` / `next()` gives the correct answer for an AOT-created generator and still
dies for an eval-created one. That is a separate, deeper defect -- the AOT Iterator protocol reads
a generator activation the interpreter-owned object does not have -- recorded in its own packet.
So the Symfony `--web` request is still blocked; this fix removes one of the two layers under it.
Evidence: scratchpad/red10/arg.php + arggen.php against php -n 8.5.10: before the change elephc printed `EVAL gen=array`, after it prints `EVAL gen=Generator`, matching php exactly on every line, exit 0. scratchpad/red10/main.php (same boundary plus the Iterator protocol) still shows `AOT valid:1 current:1 then:2` correct and the eval line still failing, which is the remaining layer. Regression slices after the change: `RUST_MIN_STACK=33554432 cargo test --test codegen_tests generator -- --test-threads=4` = 96 passed / 0 failed; `... iterable ...` = 120 passed / 13 failed, byte-identical to the pristine-tree baseline measured for the same filter, so no regression.
Verified by: php -n 8.5.10 as oracle on a 20-line pure-AOT reducer that shows the wrong kind without crashing, plus the generator and iterable codegen slices compared against a pristine-tree baseline

## Verification

scratchpad/red10/arg.php + arggen.php against php -n 8.5.10: before the change elephc printed `EVAL gen=array`, after it prints `EVAL gen=Generator`, matching php exactly on every line, exit 0. scratchpad/red10/main.php (same boundary plus the Iterator protocol) still shows `AOT valid:1 current:1 then:2` correct and the eval line still failing, which is the remaining layer. Regression slices after the change: `RUST_MIN_STACK=33554432 cargo test --test codegen_tests generator -- --test-threads=4` = 96 passed / 0 failed; `... iterable ...` = 120 passed / 13 failed, byte-identical to the pristine-tree baseline measured for the same filter, so no regression.

# Citations

[1] explicit_capture (2026-09-12T18:30:15.588Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:bug_fix:an-iterable-parameter-crossing-eval-aot-was-handed-the-boxed-cell-instead-of-the","title":"An `iterable` parameter crossing eval->AOT was handed the BOXED CELL instead of the payload, so a Generator arrived as an array","summary":"FIXED. An AOT function called from interpreted code received the boxed Mixed cell for any parameter declared iterable , where the AOT side expects the unboxed payload. A Generator passed that way arrived as an ARRAY sile","body":"FIXED. An AOT function called from interpreted code received the boxed Mixed cell for any parameter declared `iterable`, where the AOT side expects the unboxed payload. A Generator passed that way arrived as an ARRAY -- silently, with no crash and no diagnostic.\n\nTHE ONE-LINE SYMPTOM, and it needs no crash to show:\n    function what(iterable $v): string { return get_debug_type($v); }\n  php -n 8.5.10 : AOT gen=Generator arr=array   EVAL gen=Generator arr=array\n  before        : AOT gen=Generator arr=array   EVAL gen=array     arr=array\n  after         : identical to php\nThe array half was always right, which is what made the bug look like a Generator problem rather\nthan a representation problem.\n\nTHE SITE. `coerce_result_to_type` in src/codegen/runtime_callable_invoker.rs: when the source is\n`Mixed`, the match unboxes for `Array(_) | AssocArray { .. } | Object(_)` and passes through for\n`Mixed | Union(_)`. `PhpType::Iterable` matched NEITHER and fell into the trailing `_ => {}`, so\nno `__rt_mixed_unbox` was emitted and the cell pointer itself became the argument. The fix adds\n`PhpType::Iterable` to the unboxing arm, and to the allowed-target list in\n`can_coerce_result_to_type` so the coercion is actually attempted. This is the same class of\nomission as the `instanceof` one: `iterable` keeps being forgotten in matches that enumerate the\ncontainer types, because it is spelled like a scalar-ish keyword but behaves like `array|Traversable`.\n\nWHY THE ABI IS THIS WAY: coercing a gradual value TO `iterable` emits `Op::MixedUnbox`\n(src/ir_lower/gradual_coercions.rs:104) and `src/types/model.rs:120` describes the slot as a\n\"type-erased pointer (array|Traversable)\". The METHOD and CONSTRUCTOR bridges already got this\nright through a dedicated `emit_*_cast_eval_iterable_arg` (src/codegen/eval_method_helpers.rs)\nthat unboxes, accepts tags 4/5/6 and validates an object against the Traversable interface ids;\nonly the callable-invoker path -- which is how an AOT FUNCTION is exposed to the interpreter via\n`register_eval_native_function` -- was missing it.\n\nWHAT IT DOES NOT FIX, measured immediately after. The value now arrives correctly, but DRIVING an\ninterpreter-created Generator from AOT code still fails: the same probe extended to call\n`valid()` / `current()` / `next()` gives the correct answer for an AOT-created generator and still\ndies for an eval-created one. That is a separate, deeper defect -- the AOT Iterator protocol reads\na generator activation the interpreter-owned object does not have -- recorded in its own packet.\nSo the Symfony `--web` request is still blocked; this fix removes one of the two layers under it.\nEvidence: scratchpad/red10/arg.php + arggen.php against php -n 8.5.10: before the change elephc printed `EVAL gen=array`, after it prints `EVAL gen=Generator`, matching php exactly on every line, exit 0. scratchpad/red10/main.php (same boundary plus the Iterator protocol) still shows `AOT valid:1 current:1 then:2` correct and the eval line still failing, which is the remaining layer. Regression slices after the change: `RUST_MIN_STACK=33554432 cargo test --test codegen_tests generator -- --test-threads=4` = 96 passed / 0 failed; `... iterable ...` = 120 passed / 13 failed, byte-identical to the pristine-tree baseline measured for the same filter, so no regression.\nVerified by: php -n 8.5.10 as oracle on a 20-line pure-AOT reducer that shows the wrong kind without crashing, plus the generator and iterable codegen slices compared against a pristine-tree baseline","type":"bug_fix","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","symfony-web","iterable","generator","eval-bridge","abi","codegen"],"paths":["src/codegen/runtime_callable_invoker.rs","src/codegen/eval_method_helpers.rs","src/ir_lower/gradual_coercions.rs","src/types/model.rs"],"stack":["rust","php","symfony","macos-aarch64"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-12T18:30:15.588Z"}],"context":{"fact":"FIXED. An AOT function called from interpreted code received the boxed Mixed cell for any parameter declared `iterable`, where the AOT side expects the unboxed payload. A Generator passed that way arrived as an ARRAY -- silently, with no crash and no diagnostic.","verification":"scratchpad/red10/arg.php + arggen.php against php -n 8.5.10: before the change elephc printed `EVAL gen=array`, after it prints `EVAL gen=Generator`, matching php exactly on every line, exit 0. scratchpad/red10/main.php (same boundary plus the Iterator protocol) still shows `AOT valid:1 current:1 then:2` correct and the eval line still failing, which is the remaining layer. Regression slices after the change: `RUST_MIN_STACK=33554432 cargo test --test codegen_tests generator -- --test-threads=4` = 96 passed / 0 failed; `... iterable ...` = 120 passed / 13 failed, byte-identical to the pristine-tree baseline measured for the same filter, so no regression."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-12T18:30:15.588Z","path_fingerprints":[{"path":"src/codegen/runtime_callable_invoker.rs","sha256":"bbeab1c688425b052f7cc332d2014c8596ba99036073c7fc0bc012d787873d4d","size":137420},{"path":"src/codegen/eval_method_helpers.rs","sha256":"74041cde9fba2cc86a1cb927e473c9055f9b63e92823b61df042e10f4d6e97a4","size":130840},{"path":"src/ir_lower/gradual_coercions.rs","sha256":"c6f9d05fb9c27cac452f3f3fcf6a4515665f3acbaa9bbc241d11cbf751753b7f","size":5765},{"path":"src/types/model.rs","sha256":"3b375d1544a4838d7ecfc6dfb8f6e7d927d5c1a4daa827ddbac8519149c836a1","size":13695}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":8000,"discovery_tokens_estimated":true,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"stale_reasons":[],"estimated_tokens_saved":882},"created_at":"2026-09-12T18:30:15.588Z","updated_at":"2026-09-12T18:30:15.588Z","author_branch":"reconcile/dirname-symfony"}
```

