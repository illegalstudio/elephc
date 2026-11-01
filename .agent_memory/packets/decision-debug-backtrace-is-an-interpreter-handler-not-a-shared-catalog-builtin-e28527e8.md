---
type: "Decision"
title: "debug_backtrace is an interpreter handler, not a shared-catalog builtin"
description: "debug backtrace and debug print backtrace describe INTERPRETER frames, which only the interpreter has. They are wired the way the OPcache handlers next door are an arm in eval call , an arm in the by value dispatcher, an"
resource: "crates/elephc-magician/src/interpreter/builtins/core/debug_backtrace.rs"
tags: ["session-learning", "debug-backtrace", "builtin-registry", "eval-interpreter", "symfony"]
timestamp: "2026-09-06T06:02:36.454Z"
x-kage-id: "repo:lazy-petting-popcorn:decision:debug-backtrace-is-an-interpreter-handler-not-a-shared-catalog-builtin-178867455"
x-kage-type: "decision"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["crates/elephc-magician/src/interpreter/builtins/core/debug_backtrace.rs", "crates/elephc-magician/src/interpreter/expressions/calls.rs", "crates/elephc-magician/src/context/call_frames.rs", "crates/elephc-builtin-contract/src/support.rs"]
x-kage-stack: ["rust", "php", "elephc-magician", "symfony"]
---

# debug_backtrace is an interpreter handler, not a shared-catalog builtin

> debug backtrace and debug print backtrace describe INTERPRETER frames, which only the interpreter has. They are wired…

`debug_backtrace()` and `debug_print_backtrace()` describe INTERPRETER frames, which only the interpreter has. They are wired the way the OPcache handlers next door are -- an arm in `eval_call`, an arm in the by-value dispatcher, and an arm in the `function_exists` probe -- rather than through the shared builtin contract.

A catalog entry would also claim an AOT implementation, and a compiled binary has no interpreter frames to report. It would additionally require updating the hard-coded backend-support counts in the contract crate's tests and regenerating the builtin docs, which needs `cargo build --example gen_builtins` plus the docs extractor.

The blocking mechanic to remember: `EvalBuiltinSpec::from_binding` PANICS on an unknown contract id, so the `eval_builtin!` macro cannot be used at all for a builtin that is not in the catalog. Adding a builtin from the interpreter alone means the three hand-wired arms.

The frames themselves keep PHP's split -- the CALLEE's name with the CALLER's file and line, captured together before the body can move the call site -- and argument values are held by handle without retaining, because a frame lives strictly inside the call it describes.
Evidence: The Symfony crux this exists for: ClassExistenceResource::throwOnRequiredClass reads the frame above itself and returns silently when it names a class probe with no `class` key. `spl_autoload_register('probeAutoload'); class_exists('MissingProbeOne');` gives `2:class_exists:noclass:MissingProbeOne:probeAutoload:MissingProbeOne` under both php -n 8.5.6 and the interpreter. An empty trace would take the throwing branch instead.
Verified by: Commit 8748d52b45, five tests measured against php -n 8.5.6, suite unchanged at 16 pre-existing failures.

## Verification

The Symfony crux this exists for: ClassExistenceResource::throwOnRequiredClass reads the frame above itself and returns silently when it names a class probe with no `class` key. `spl_autoload_register('probeAutoload'); class_exists('MissingProbeOne');` gives `2:class_exists:noclass:MissingProbeOne:probeAutoload:MissingProbeOne` under both php -n 8.5.6 and the interpreter. An empty trace would take the throwing branch instead.

# Citations

[1] explicit_capture (2026-09-06T06:02:36.454Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:decision:debug-backtrace-is-an-interpreter-handler-not-a-shared-catalog-builtin-178867455","title":"debug_backtrace is an interpreter handler, not a shared-catalog builtin","summary":"debug backtrace and debug print backtrace describe INTERPRETER frames, which only the interpreter has. They are wired the way the OPcache handlers next door are an arm in eval call , an arm in the by value dispatcher, an","body":"`debug_backtrace()` and `debug_print_backtrace()` describe INTERPRETER frames, which only the interpreter has. They are wired the way the OPcache handlers next door are -- an arm in `eval_call`, an arm in the by-value dispatcher, and an arm in the `function_exists` probe -- rather than through the shared builtin contract.\n\nA catalog entry would also claim an AOT implementation, and a compiled binary has no interpreter frames to report. It would additionally require updating the hard-coded backend-support counts in the contract crate's tests and regenerating the builtin docs, which needs `cargo build --example gen_builtins` plus the docs extractor.\n\nThe blocking mechanic to remember: `EvalBuiltinSpec::from_binding` PANICS on an unknown contract id, so the `eval_builtin!` macro cannot be used at all for a builtin that is not in the catalog. Adding a builtin from the interpreter alone means the three hand-wired arms.\n\nThe frames themselves keep PHP's split -- the CALLEE's name with the CALLER's file and line, captured together before the body can move the call site -- and argument values are held by handle without retaining, because a frame lives strictly inside the call it describes.\nEvidence: The Symfony crux this exists for: ClassExistenceResource::throwOnRequiredClass reads the frame above itself and returns silently when it names a class probe with no `class` key. `spl_autoload_register('probeAutoload'); class_exists('MissingProbeOne');` gives `2:class_exists:noclass:MissingProbeOne:probeAutoload:MissingProbeOne` under both php -n 8.5.6 and the interpreter. An empty trace would take the throwing branch instead.\nVerified by: Commit 8748d52b45, five tests measured against php -n 8.5.6, suite unchanged at 16 pre-existing failures.","type":"decision","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","debug-backtrace","builtin-registry","eval-interpreter","symfony"],"paths":["crates/elephc-magician/src/interpreter/builtins/core/debug_backtrace.rs","crates/elephc-magician/src/interpreter/expressions/calls.rs","crates/elephc-magician/src/context/call_frames.rs","crates/elephc-builtin-contract/src/support.rs"],"stack":["rust","php","elephc-magician","symfony"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-06T06:02:36.454Z"}],"context":{"fact":"`debug_backtrace()` and `debug_print_backtrace()` describe INTERPRETER frames, which only the interpreter has. They are wired the way the OPcache handlers next door are -- an arm in `eval_call`, an arm in the by-value dispatcher, and an arm in the `function_exists` probe -- rather than through the shared builtin contract.","verification":"The Symfony crux this exists for: ClassExistenceResource::throwOnRequiredClass reads the frame above itself and returns silently when it names a class probe with no `class` key. `spl_autoload_register('probeAutoload'); class_exists('MissingProbeOne');` gives `2:class_exists:noclass:MissingProbeOne:probeAutoload:MissingProbeOne` under both php -n 8.5.6 and the interpreter. An empty trace would take the throwing branch instead."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-06T06:02:36.454Z","path_fingerprints":[{"path":"crates/elephc-magician/src/interpreter/builtins/core/debug_backtrace.rs","sha256":"10b181e08513183f75e998d86ca661b4599917e3be06bc6a5242842c9b820166","size":9604},{"path":"crates/elephc-magician/src/interpreter/expressions/calls.rs","sha256":"cc0e1e1400f0ca0120b585f5ddece1ba1ac5e1f97460877120e38b2a73b6ede9","size":10414},{"path":"crates/elephc-magician/src/context/call_frames.rs","sha256":"a3ac8d9090915d5b2caf45bd0c5e5790cc10a92c8ee3c419d3fdf7caf2846da1","size":6406},{"path":"crates/elephc-builtin-contract/src/support.rs","sha256":"18f9e72382be88e66f771d63dbc7fb2699491ed593c3d1d4e7fa6e2ab640e78b","size":13009}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":4000,"discovery_tokens_estimated":true,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"stale_reasons":[],"estimated_tokens_saved":440},"created_at":"2026-09-06T06:02:36.454Z","updated_at":"2026-09-06T06:02:36.454Z","author_branch":"reconcile/dirname-symfony"}
```

