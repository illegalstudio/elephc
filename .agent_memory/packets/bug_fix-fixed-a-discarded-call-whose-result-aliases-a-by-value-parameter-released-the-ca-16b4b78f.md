---
type: "Bug Fix"
title: "FIXED: a discarded call whose result aliases a by-value PARAMETER released the caller's variable, because a return transfers ownership only when the activation scope owned the cell"
description: "FIXED. f $x ; as a STATEMENT destroyed the caller's $x whenever f returned the parameter it was given. THE MECHANISM. A return transfers ownership by release activation scope SKIPPING the returned cell — which only hands"
resource: "crates/elephc-magician/src/interpreter/statements/dispatch.rs"
tags: ["session-learning", "eval-bridge", "ownership", "refcount", "symfony", "http-kernel"]
timestamp: "2026-09-12T21:11:35.830Z"
x-kage-id: "repo:lazy-petting-popcorn:bug_fix:fixed-a-discarded-call-whose-result-aliases-a-by-value-parameter-released-the-ca"
x-kage-type: "bug_fix"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["crates/elephc-magician/src/interpreter/statements/dispatch.rs", "crates/elephc-magician/src/interpreter/statements/reference_writeback.rs", "crates/elephc-magician/src/interpreter/dynamic_functions/closure_execution.rs"]
x-kage-stack: ["rust", "php"]
---

# FIXED: a discarded call whose result aliases a by-value PARAMETER released the caller's variable, because a return transfers ownership only when the activation scope owned the cell

> FIXED. f $x ; as a STATEMENT destroyed the caller's $x whenever f returned the parameter it was given. THE MECHANISM.…

FIXED. `f($x);` as a STATEMENT destroyed the caller's `$x` whenever `f` returned the parameter it was given.

THE MECHANISM. A return transfers ownership by `release_activation_scope` SKIPPING the returned cell — which only hands the caller a reference when the activation scope actually owned one. `bind_method_scope_args` binds every by-value parameter `ScopeCellOwnership::Borrowed` on purpose, so `return $param;` handed back a BORROWED cell. Every caller treats a call result as owned — `eval_expr_result_aliases_storage` deliberately does not list a call — so the statement-level discard released a reference nobody had taken, and the caller's variable died.

THE MATRIX that located it, each case its own file because the first failure kills the process (scratchpad/red16): result discarded + aliases the argument = BROKEN, both typed and untyped; result ASSIGNED = fine; callee returns something else = fine; callee ignores the argument = fine.

THE CARVE-OUT THAT IS NOT OPTIONAL. `$this` must stay excluded. The receiver already has its own compensation on the CALL side (a method result aliasing the receiver is retained there), so retaining on return as well hands back two references and a discarded `$obj->fluent();` never runs the object's destructor — measured as three `interpreter::tests::fluent_method_discard` failures, which is exactly what that suite is for. The same file already carried the `static` case for the same underlying reason: a static local has no scope entry to skip either.

WHY IT MATTERED. Symfony's `HttpKernel::filterResponse` is `$this->dispatcher->dispatch($event, ...);` as a statement, and `EventDispatcher::dispatch()` returns its `$event` parameter. The local `$event` was then a freed cell, and `$event->getResponse()` three lines later read a type tag out of freed memory — `phase=method_call_non_object ... tag=Some(4503492208)`, a "tag" far outside 0..10, which is the tell for a released cell rather than a wrong type.
Evidence: scratchpad/red16: `case_discarded_typed` and `case_discarded_untyped` printed a fatal before the fix and `response` after; `case_used`, `case_no_alias` and `case_other_object` were correct throughout. Symfony --web advanced from `unsupported MethodCall expression` at HttpKernel.php:225 to the next rung.
Verified by: cargo test -p elephc-magician: 1687 passed / 2 failed, identical to the pre-change baseline (the two failures are pre-existing). Without the `$this` carve-out the same run was 1684/5.

## Verification

scratchpad/red16: `case_discarded_typed` and `case_discarded_untyped` printed a fatal before the fix and `response` after; `case_used`, `case_no_alias` and `case_other_object` were correct throughout. Symfony --web advanced from `unsupported MethodCall expression` at HttpKernel.php:225 to the next rung.

# Citations

[1] explicit_capture (2026-09-12T21:11:35.830Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:bug_fix:fixed-a-discarded-call-whose-result-aliases-a-by-value-parameter-released-the-ca","title":"FIXED: a discarded call whose result aliases a by-value PARAMETER released the caller's variable, because a return transfers ownership only when the activation scope owned the cell","summary":"FIXED. f $x ; as a STATEMENT destroyed the caller's $x whenever f returned the parameter it was given. THE MECHANISM. A return transfers ownership by release activation scope SKIPPING the returned cell — which only hands","body":"FIXED. `f($x);` as a STATEMENT destroyed the caller's `$x` whenever `f` returned the parameter it was given.\n\nTHE MECHANISM. A return transfers ownership by `release_activation_scope` SKIPPING the returned cell — which only hands the caller a reference when the activation scope actually owned one. `bind_method_scope_args` binds every by-value parameter `ScopeCellOwnership::Borrowed` on purpose, so `return $param;` handed back a BORROWED cell. Every caller treats a call result as owned — `eval_expr_result_aliases_storage` deliberately does not list a call — so the statement-level discard released a reference nobody had taken, and the caller's variable died.\n\nTHE MATRIX that located it, each case its own file because the first failure kills the process (scratchpad/red16): result discarded + aliases the argument = BROKEN, both typed and untyped; result ASSIGNED = fine; callee returns something else = fine; callee ignores the argument = fine.\n\nTHE CARVE-OUT THAT IS NOT OPTIONAL. `$this` must stay excluded. The receiver already has its own compensation on the CALL side (a method result aliasing the receiver is retained there), so retaining on return as well hands back two references and a discarded `$obj->fluent();` never runs the object's destructor — measured as three `interpreter::tests::fluent_method_discard` failures, which is exactly what that suite is for. The same file already carried the `static` case for the same underlying reason: a static local has no scope entry to skip either.\n\nWHY IT MATTERED. Symfony's `HttpKernel::filterResponse` is `$this->dispatcher->dispatch($event, ...);` as a statement, and `EventDispatcher::dispatch()` returns its `$event` parameter. The local `$event` was then a freed cell, and `$event->getResponse()` three lines later read a type tag out of freed memory — `phase=method_call_non_object ... tag=Some(4503492208)`, a \"tag\" far outside 0..10, which is the tell for a released cell rather than a wrong type.\nEvidence: scratchpad/red16: `case_discarded_typed` and `case_discarded_untyped` printed a fatal before the fix and `response` after; `case_used`, `case_no_alias` and `case_other_object` were correct throughout. Symfony --web advanced from `unsupported MethodCall expression` at HttpKernel.php:225 to the next rung.\nVerified by: cargo test -p elephc-magician: 1687 passed / 2 failed, identical to the pre-change baseline (the two failures are pre-existing). Without the `$this` carve-out the same run was 1684/5.","type":"bug_fix","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","eval-bridge","ownership","refcount","symfony","http-kernel"],"paths":["crates/elephc-magician/src/interpreter/statements/dispatch.rs","crates/elephc-magician/src/interpreter/statements/reference_writeback.rs","crates/elephc-magician/src/interpreter/dynamic_functions/closure_execution.rs"],"stack":["rust","php"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-12T21:11:35.830Z"}],"context":{"fact":"FIXED. `f($x);` as a STATEMENT destroyed the caller's `$x` whenever `f` returned the parameter it was given.","verification":"scratchpad/red16: `case_discarded_typed` and `case_discarded_untyped` printed a fatal before the fix and `response` after; `case_used`, `case_no_alias` and `case_other_object` were correct throughout. Symfony --web advanced from `unsupported MethodCall expression` at HttpKernel.php:225 to the next rung."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-12T21:11:35.830Z","path_fingerprints":[{"path":"crates/elephc-magician/src/interpreter/statements/dispatch.rs","sha256":"1ec601d9ff723ea0f8c3b6ae06d8d18347de3d476a14fcb435c5c4d2240f2cb6","size":23207},{"path":"crates/elephc-magician/src/interpreter/statements/reference_writeback.rs","sha256":"e00d5a72b978b3379d325d4fe898d00eca309be0b579c83e9f56a01525607a71","size":21689},{"path":"crates/elephc-magician/src/interpreter/dynamic_functions/closure_execution.rs","sha256":"06a3fe94c857d8b1c06e539dccbc254355a552fc7dc0dd628c8fb0d576b23ae0","size":33943}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":160000,"discovery_tokens_estimated":false,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"stale_reasons":[],"estimated_tokens_saved":621},"created_at":"2026-09-12T21:11:35.830Z","updated_at":"2026-09-12T21:11:35.830Z","author_branch":"reconcile/dirname-symfony"}
```

