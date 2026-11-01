---
type: "Bug Fix"
title: "FIXED: a NULL-handle bridge call minted a throwaway eval context per call, so every builtin backed by per-context state lost it between two calls"
description: "FIXED. elephc eval call function{, array} reached with a null ctx did fallback context = ElephcEvalContext::new on the STACK and dropped it when the call returned. A null handle is the normal way generated code says \"no"
resource: "crates/elephc-magician/src/ffi/context.rs"
tags: ["session-learning", "eval-bridge", "ffi", "context", "hash", "stream-resources", "symfony"]
timestamp: "2026-09-12T21:12:05.725Z"
x-kage-id: "repo:lazy-petting-popcorn:bug_fix:fixed-a-null-handle-bridge-call-minted-a-throwaway-eval-context-per-call-so-ever"
x-kage-type: "bug_fix"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["crates/elephc-magician/src/ffi/context.rs", "crates/elephc-magician/src/ffi/function_calls.rs", "crates/elephc-magician/src/stream_resources.rs"]
x-kage-stack: ["rust", "php"]
---

# FIXED: a NULL-handle bridge call minted a throwaway eval context per call, so every builtin backed by per-context state lost it between two calls

> FIXED. elephc eval call function{, array} reached with a null ctx did fallback context = ElephcEvalContext::new on th…

FIXED. `__elephc_eval_call_function{,_array}` reached with a null `ctx` did `fallback_context = ElephcEvalContext::new()` on the STACK and dropped it when the call returned. A null handle is the normal way generated code says "no context declares this name, resolve it yourself" — it is how the backtrace pair, the OPcache family, the date aliases and every interpreter-implemented builtin are reached — so this was not a rare path.

THE CONSEQUENCE: every builtin whose state lives ON the context died with the call that created it. `hash_init()` registered its context in one throwaway `stream_resources` table and `hash_final()` looked it up in another, empty one, which surfaced as a bare `Fatal error: eval() runtime failed` with nothing to connect the two calls. Stream resources, output buffers and every other per-context table have the same exposure.

THE FIX: `ffi::context::shared_null_handle_context()` — one leaked context per process, re-synced with the global AOT metadata on each use. That is the faithful scope: these tables are REQUEST state in PHP, not call state, and `--web` forks a worker per request.

HOW IT PRESENTED, and why the obvious reducer did not reproduce it: a program where BOTH `hash_init` and `hash_final` are interpreted shares one context and works (scratchpad/red21 passes before and after). It only breaks when the CALLS come from compiled code through the bridge, which is what an autoloaded class gives you.

RELATED, same family, already documented separately: eval class metadata is process-global while static property VALUES are per-context.
Evidence: Symfony --web: before, the request died at `phase=dynamic_function_call name="symfony\component\config\resource\hash_final" status=RuntimeFatal` after 163 trace lines; after, it runs 2545 trace lines and reaches the next rung. codegen_tests generator 96/0, iterable 120/13, exception 145/2, instanceof 59/2 and elephc-magician 1687/2 all unchanged.
Verified by: The Symfony --web trace advancing past hash_final, plus the five unchanged test slices.

## Verification

Symfony --web: before, the request died at `phase=dynamic_function_call name="symfony\component\config\resource\hash_final" status=RuntimeFatal` after 163 trace lines; after, it runs 2545 trace lines and reaches the next rung. codegen_tests generator 96/0, iterable 120/13, exception 145/2, instanceof 59/2 and elephc-magician 1687/2 all unchanged.

# Citations

[1] explicit_capture (2026-09-12T21:12:05.725Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:bug_fix:fixed-a-null-handle-bridge-call-minted-a-throwaway-eval-context-per-call-so-ever","title":"FIXED: a NULL-handle bridge call minted a throwaway eval context per call, so every builtin backed by per-context state lost it between two calls","summary":"FIXED. elephc eval call function{, array} reached with a null ctx did fallback context = ElephcEvalContext::new on the STACK and dropped it when the call returned. A null handle is the normal way generated code says \"no","body":"FIXED. `__elephc_eval_call_function{,_array}` reached with a null `ctx` did `fallback_context = ElephcEvalContext::new()` on the STACK and dropped it when the call returned. A null handle is the normal way generated code says \"no context declares this name, resolve it yourself\" — it is how the backtrace pair, the OPcache family, the date aliases and every interpreter-implemented builtin are reached — so this was not a rare path.\n\nTHE CONSEQUENCE: every builtin whose state lives ON the context died with the call that created it. `hash_init()` registered its context in one throwaway `stream_resources` table and `hash_final()` looked it up in another, empty one, which surfaced as a bare `Fatal error: eval() runtime failed` with nothing to connect the two calls. Stream resources, output buffers and every other per-context table have the same exposure.\n\nTHE FIX: `ffi::context::shared_null_handle_context()` — one leaked context per process, re-synced with the global AOT metadata on each use. That is the faithful scope: these tables are REQUEST state in PHP, not call state, and `--web` forks a worker per request.\n\nHOW IT PRESENTED, and why the obvious reducer did not reproduce it: a program where BOTH `hash_init` and `hash_final` are interpreted shares one context and works (scratchpad/red21 passes before and after). It only breaks when the CALLS come from compiled code through the bridge, which is what an autoloaded class gives you.\n\nRELATED, same family, already documented separately: eval class metadata is process-global while static property VALUES are per-context.\nEvidence: Symfony --web: before, the request died at `phase=dynamic_function_call name=\"symfony\\component\\config\\resource\\hash_final\" status=RuntimeFatal` after 163 trace lines; after, it runs 2545 trace lines and reaches the next rung. codegen_tests generator 96/0, iterable 120/13, exception 145/2, instanceof 59/2 and elephc-magician 1687/2 all unchanged.\nVerified by: The Symfony --web trace advancing past hash_final, plus the five unchanged test slices.","type":"bug_fix","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","eval-bridge","ffi","context","hash","stream-resources","symfony"],"paths":["crates/elephc-magician/src/ffi/context.rs","crates/elephc-magician/src/ffi/function_calls.rs","crates/elephc-magician/src/stream_resources.rs"],"stack":["rust","php"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-12T21:12:05.725Z"}],"context":{"fact":"FIXED. `__elephc_eval_call_function{,_array}` reached with a null `ctx` did `fallback_context = ElephcEvalContext::new()` on the STACK and dropped it when the call returned. A null handle is the normal way generated code says \"no context declares this name, resolve it yourself\" — it is how the backtrace pair, the OPcache family, the date aliases and every interpreter-implemented builtin are reached — so this was not a rare path.","verification":"Symfony --web: before, the request died at `phase=dynamic_function_call name=\"symfony\\component\\config\\resource\\hash_final\" status=RuntimeFatal` after 163 trace lines; after, it runs 2545 trace lines and reaches the next rung. codegen_tests generator 96/0, iterable 120/13, exception 145/2, instanceof 59/2 and elephc-magician 1687/2 all unchanged."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-12T21:12:05.725Z","path_fingerprints":[{"path":"crates/elephc-magician/src/ffi/context.rs","sha256":"d3baf6a33e5cc89d8234e4b409ba866c16f51407cb4509b88f2b7f985612413d","size":26454},{"path":"crates/elephc-magician/src/ffi/function_calls.rs","sha256":"c686f3b43df2f387b2d27911e05ed316bbde533a27678ba06eeaa4111ece1467","size":11700},{"path":"crates/elephc-magician/src/stream_resources.rs","sha256":"89294b4efa7fac1e786023188095ccd1ca815a8d07b0a8a61a2d5f2a01dcd6d4","size":4224}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":120000,"discovery_tokens_estimated":false,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"stale_reasons":[],"estimated_tokens_saved":512},"created_at":"2026-09-12T21:12:05.725Z","updated_at":"2026-09-12T21:12:05.725Z","author_branch":"reconcile/dirname-symfony"}
```

