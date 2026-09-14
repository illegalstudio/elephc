---
type: "Decision"
title: "opcache_reset() has TWO independent latches, and only the eval one reached the cache"
description: "opcache reset is answered by two different objects and it is easy to fix one and believe the feature works. 1. THE NATIVE LATCH — a PHP static in the injected prelude, reached through elephc opcache restart pending $sche"
resource: "crates/elephc-magician/src/script_cache/store.rs"
tags: ["session-learning", "opcache", "opcache_reset", "restart", "web", "request-boundary", "php-parity"]
timestamp: "2026-09-12T18:11:53.225Z"
x-kage-id: "repo:sparkling-jingling-flute:decision:opcache-reset-has-two-independent-latches-and-only-the-eval-one-reached-the-cach"
x-kage-type: "decision"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["crates/elephc-magician/src/script_cache/store.rs", "crates/elephc-magician/src/ffi/context.rs", "src/codegen/frame.rs", "src/opcache_prelude/build.rs", "tests/web_session_tests.rs"]
x-kage-stack: ["rust", "php"]
---

# opcache_reset() has TWO independent latches, and only the eval one reached the cache

> opcache reset is answered by two different objects and it is easy to fix one and believe the feature works. 1. THE NA…

`opcache_reset()` is answered by two different objects and it is easy to fix one and believe the feature works.

1. THE NATIVE LATCH — a PHP static in the injected prelude, reached through `__elephc_opcache_restart_pending($schedule)`. It gives the once-then-false return value and feeds `restart_pending` in the status array.
2. THE RUNTIME CACHE'S LATCH — `script_cache::schedule_restart` in elephc-magician.

Until this change `schedule_restart` had EXACTLY ONE CALLER: the eval interpreter's `opcache_reset` builtin. So a natively compiled `opcache_reset()` — which is where ordinary code calls it — moved only the reported latch and left the dynamic tier serving its entries. `grep -rn schedule_restart` is how that was found; a test that called `opcache_reset()` natively and expected a flush simply saw nothing happen, with no error. The prelude now also calls a new internal `__elephc_opcache_rt_reset` builtin.

THE DEFERRAL ITSELF. php-src SCHEDULES and restarts at the next request. VERIFIED on reference PHP 8.5.10 that within the scheduling request FOUR values stay put, not just the entries: `opcache_is_script_cached()` still answers true, `num_cached_scripts` is unchanged, and `manual_restarts` and `last_restart_time` are both still 0. `schedule_restart` therefore only latches; `apply_pending_restart` clears the entries, counts the restart, stamps the time and releases the latch so a later request can schedule its own.

WHERE THE REQUEST BOUNDARY WAS ALREADY: `codegen::frame::emit_web_handler_prologue` emits per-request resets (`__rt_web_reset`) before the body. That is the spot php-src restarts at, so the apply call sits beside it, gated on `eval_bridge` by the usual pay-for-use rule. A CLI program never performs the restart, which is correct rather than missing — reference would restart at a next request a CLI process does not have.

HOW TO OBSERVE IT, because the obvious assertions do not discriminate: `num_cached_scripts` is manifest + runtime, so it reads 2 both when the entry survives and when it is refilled. The signal is hits/misses across two requests against ONE worker (`--workers 1`): with a reset, request 2 reports `h=0 m=2 r=1` (the entry was gone, so the include missed again); without one it reports `h=1 m=1`. The `--web` default isolation is `Worker`, a persistent process, so the cache genuinely survives between requests — confirmed directly before relying on it.
Evidence: Reference PHP 8.5.10 probed for the four post-reset values. The native-vs-eval split was found by a two-request --web test that showed manual_restarts stuck at 0, then by grepping schedule_restart's callers. Cache persistence across requests confirmed by a probe reporting h=1 m=1 on the second request.
Verified by: cargo test --workspace --lib --no-fail-fast (3551 passed, 0 failed), the nine opcache suites, and web_session_tests (50 passed) including the two-request boundary test

## Verification

Reference PHP 8.5.10 probed for the four post-reset values. The native-vs-eval split was found by a two-request --web test that showed manual_restarts stuck at 0, then by grepping schedule_restart's callers. Cache persistence across requests confirmed by a probe reporting h=1 m=1 on the second request.

# Citations

[1] explicit_capture (2026-09-12T18:11:53.225Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:sparkling-jingling-flute:decision:opcache-reset-has-two-independent-latches-and-only-the-eval-one-reached-the-cach","title":"opcache_reset() has TWO independent latches, and only the eval one reached the cache","summary":"opcache reset is answered by two different objects and it is easy to fix one and believe the feature works. 1. THE NATIVE LATCH — a PHP static in the injected prelude, reached through elephc opcache restart pending $sche","body":"`opcache_reset()` is answered by two different objects and it is easy to fix one and believe the feature works.\n\n1. THE NATIVE LATCH — a PHP static in the injected prelude, reached through `__elephc_opcache_restart_pending($schedule)`. It gives the once-then-false return value and feeds `restart_pending` in the status array.\n2. THE RUNTIME CACHE'S LATCH — `script_cache::schedule_restart` in elephc-magician.\n\nUntil this change `schedule_restart` had EXACTLY ONE CALLER: the eval interpreter's `opcache_reset` builtin. So a natively compiled `opcache_reset()` — which is where ordinary code calls it — moved only the reported latch and left the dynamic tier serving its entries. `grep -rn schedule_restart` is how that was found; a test that called `opcache_reset()` natively and expected a flush simply saw nothing happen, with no error. The prelude now also calls a new internal `__elephc_opcache_rt_reset` builtin.\n\nTHE DEFERRAL ITSELF. php-src SCHEDULES and restarts at the next request. VERIFIED on reference PHP 8.5.10 that within the scheduling request FOUR values stay put, not just the entries: `opcache_is_script_cached()` still answers true, `num_cached_scripts` is unchanged, and `manual_restarts` and `last_restart_time` are both still 0. `schedule_restart` therefore only latches; `apply_pending_restart` clears the entries, counts the restart, stamps the time and releases the latch so a later request can schedule its own.\n\nWHERE THE REQUEST BOUNDARY WAS ALREADY: `codegen::frame::emit_web_handler_prologue` emits per-request resets (`__rt_web_reset`) before the body. That is the spot php-src restarts at, so the apply call sits beside it, gated on `eval_bridge` by the usual pay-for-use rule. A CLI program never performs the restart, which is correct rather than missing — reference would restart at a next request a CLI process does not have.\n\nHOW TO OBSERVE IT, because the obvious assertions do not discriminate: `num_cached_scripts` is manifest + runtime, so it reads 2 both when the entry survives and when it is refilled. The signal is hits/misses across two requests against ONE worker (`--workers 1`): with a reset, request 2 reports `h=0 m=2 r=1` (the entry was gone, so the include missed again); without one it reports `h=1 m=1`. The `--web` default isolation is `Worker`, a persistent process, so the cache genuinely survives between requests — confirmed directly before relying on it.\nEvidence: Reference PHP 8.5.10 probed for the four post-reset values. The native-vs-eval split was found by a two-request --web test that showed manual_restarts stuck at 0, then by grepping schedule_restart's callers. Cache persistence across requests confirmed by a probe reporting h=1 m=1 on the second request.\nVerified by: cargo test --workspace --lib --no-fail-fast (3551 passed, 0 failed), the nine opcache suites, and web_session_tests (50 passed) including the two-request boundary test","type":"decision","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","opcache","opcache_reset","restart","web","request-boundary","php-parity"],"paths":["crates/elephc-magician/src/script_cache/store.rs","crates/elephc-magician/src/ffi/context.rs","src/codegen/frame.rs","src/opcache_prelude/build.rs","tests/web_session_tests.rs"],"stack":["rust","php"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-12T18:11:53.225Z"}],"context":{"fact":"`opcache_reset()` is answered by two different objects and it is easy to fix one and believe the feature works.","verification":"Reference PHP 8.5.10 probed for the four post-reset values. The native-vs-eval split was found by a two-request --web test that showed manual_restarts stuck at 0, then by grepping schedule_restart's callers. Cache persistence across requests confirmed by a probe reporting h=1 m=1 on the second request."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-12T18:11:53.225Z","path_fingerprints":[{"path":"crates/elephc-magician/src/script_cache/store.rs","sha256":"158e3390aab57641bfe725d50c55f14d1c1c2e2e7cf20f3a3e93704dadc85f41","size":16282},{"path":"crates/elephc-magician/src/ffi/context.rs","sha256":"e1fc3e82792119d0e518207a4e5be1d98b680df9b953c8e0dd5b94e50f18d28d","size":21402},{"path":"src/codegen/frame.rs","sha256":"0cfc74e3d25084ca270ca6a29bc3e718502da127805d9b4c03c4a4961774759f","size":92013},{"path":"src/opcache_prelude/build.rs","sha256":"ef5718e5a381f3866ec87b2da89a99b3ce43393e0b6143ae1d661eda8b766fe7","size":78143},{"path":"tests/web_session_tests.rs","sha256":"b77d9c060f156fb58272cf21aad8a09aac4443817d2a9f630aecbcdfca475679","size":88283}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":4000,"discovery_tokens_estimated":true,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"estimated_tokens_saved":729},"created_at":"2026-09-12T18:11:53.225Z","updated_at":"2026-09-12T18:12:29.319Z","author_branch":"feat/opcache-runtime-cache"}
```

