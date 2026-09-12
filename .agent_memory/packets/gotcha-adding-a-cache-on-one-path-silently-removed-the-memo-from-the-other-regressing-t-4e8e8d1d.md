---
type: "Gotcha"
title: "Adding a cache on one path silently removed the memo from the other, regressing the default binary 3.4x"
description: "When the runtime script cache was introduced, load script routed BOTH the cached and the uncached path through the new segment script , which parsed blocks directly with parser::parse fragment . That silently dropped the"
resource: "crates/elephc-magician/src/script_cache/segments.rs"
tags: ["session-learning", "opcache", "script-cache", "regression", "benchmark", "performance"]
timestamp: "2026-09-11T15:09:33.291Z"
x-kage-id: "repo:sparkling-jingling-flute:gotcha:adding-a-cache-on-one-path-silently-removed-the-memo-from-the-other-regressing-t"
x-kage-type: "gotcha"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["crates/elephc-magician/src/script_cache/segments.rs", "crates/elephc-magician/src/script_cache/store.rs"]
x-kage-stack: ["rust"]
---

# Adding a cache on one path silently removed the memo from the other, regressing the default binary 3.4x

> When the runtime script cache was introduced, load script routed BOTH the cached and the uncached path through the ne…

When the runtime script cache was introduced, `load_script` routed BOTH the cached and the uncached path through the new `segment_script`, which parsed blocks directly with `parser::parse_fragment`. That silently dropped the byte-keyed `parse_fragment_cached` memo the inline loop had always used on the uncached path. Result: the DEFAULT CLI binary — the configuration that was supposed to be byte-identical to before — went from 1580 us to 5377 us per 63 KiB include, a 3.4x REGRESSION, while the newly cached path looked spectacular.

Nothing in the test suite caught it. It was found only by re-running the SAME benchmark on the DEFAULT build after the change, which is the measurement that had no reason to move.

The fix is `ScriptSegment`'s `ParseMode`: `Memoized` when the caller discards the result (cache off, so the byte-keyed memo is the only memo there is), `Fresh` when the caller keeps it (cache on, where the script entry IS the memo and routing through the byte cache would only pin a second copy). It is load-bearing, not a tuning knob.

GENERAL LESSON: when a change adds a fast path, measure the SLOW path too. A benchmark that only covers the configuration you improved cannot see that you broke the one you did not.
Evidence: Same 63 KiB fixture: baseline (pre-change) 1580 us, post-change default CLI 5377 us, after the ParseMode fix 1474 us (back within noise of baseline), cache-on 42.6 us.
Verified by: Three compiled probe runs: pre-change binary, post-change default binary, post-fix default binary.

## Verification

Same 63 KiB fixture: baseline (pre-change) 1580 us, post-change default CLI 5377 us, after the ParseMode fix 1474 us (back within noise of baseline), cache-on 42.6 us.

# Citations

[1] explicit_capture (2026-09-11T15:09:33.291Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:sparkling-jingling-flute:gotcha:adding-a-cache-on-one-path-silently-removed-the-memo-from-the-other-regressing-t","title":"Adding a cache on one path silently removed the memo from the other, regressing the default binary 3.4x","summary":"When the runtime script cache was introduced, load script routed BOTH the cached and the uncached path through the new segment script , which parsed blocks directly with parser::parse fragment . That silently dropped the","body":"When the runtime script cache was introduced, `load_script` routed BOTH the cached and the uncached path through the new `segment_script`, which parsed blocks directly with `parser::parse_fragment`. That silently dropped the byte-keyed `parse_fragment_cached` memo the inline loop had always used on the uncached path. Result: the DEFAULT CLI binary — the configuration that was supposed to be byte-identical to before — went from 1580 us to 5377 us per 63 KiB include, a 3.4x REGRESSION, while the newly cached path looked spectacular.\n\nNothing in the test suite caught it. It was found only by re-running the SAME benchmark on the DEFAULT build after the change, which is the measurement that had no reason to move.\n\nThe fix is `ScriptSegment`'s `ParseMode`: `Memoized` when the caller discards the result (cache off, so the byte-keyed memo is the only memo there is), `Fresh` when the caller keeps it (cache on, where the script entry IS the memo and routing through the byte cache would only pin a second copy). It is load-bearing, not a tuning knob.\n\nGENERAL LESSON: when a change adds a fast path, measure the SLOW path too. A benchmark that only covers the configuration you improved cannot see that you broke the one you did not.\nEvidence: Same 63 KiB fixture: baseline (pre-change) 1580 us, post-change default CLI 5377 us, after the ParseMode fix 1474 us (back within noise of baseline), cache-on 42.6 us.\nVerified by: Three compiled probe runs: pre-change binary, post-change default binary, post-fix default binary.","type":"gotcha","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","opcache","script-cache","regression","benchmark","performance"],"paths":["crates/elephc-magician/src/script_cache/segments.rs","crates/elephc-magician/src/script_cache/store.rs"],"stack":["rust"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-11T15:09:33.291Z"}],"context":{"fact":"When the runtime script cache was introduced, `load_script` routed BOTH the cached and the uncached path through the new `segment_script`, which parsed blocks directly with `parser::parse_fragment`. That silently dropped the byte-keyed `parse_fragment_cached` memo the inline loop had always used on the uncached path. Result: the DEFAULT CLI binary — the configuration that was supposed to be byte-identical to before — went from 1580 us to 5377 us per 63 KiB include, a 3.4x REGRESSION, while the newly cached path looked spectacular.","verification":"Same 63 KiB fixture: baseline (pre-change) 1580 us, post-change default CLI 5377 us, after the ParseMode fix 1474 us (back within noise of baseline), cache-on 42.6 us."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-11T15:09:33.291Z","path_fingerprints":[{"path":"crates/elephc-magician/src/script_cache/segments.rs","sha256":"f29849912d3d964d59d7710f752351f41b3cf0dfc2b02d442a8e270e01b5b8a9","size":9549},{"path":"crates/elephc-magician/src/script_cache/store.rs","sha256":"55b1b6d750069775aae459ba6ae7266db1c46250abf9cca11abbdf6cee0807d4","size":14452}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":8000,"discovery_tokens_estimated":true,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"estimated_tokens_saved":382,"stale":true,"stale_reasons":["linked path changed since memory was verified: crates/elephc-magician/src/script_cache/store.rs"],"suggested_action":"update"},"created_at":"2026-09-11T15:09:33.291Z","updated_at":"2026-09-12T18:12:29.322Z","author_branch":"feat/opcache-runtime-cache"}
```

