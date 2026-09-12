---
type: "Bug Fix"
title: "Removing a cache entry before refilling it leaked its footprint: the budget counted the same bytes twice"
description: "script cache::compile file removed the existing entry before calling fill entry , to clear the forced invalidate latch. HashMap::remove does not return the entry's bytes to used memory , and fill entry computes replacing"
resource: "crates/elephc-magician/src/script_cache/store.rs"
tags: ["session-learning", "opcache", "script-cache", "accounting", "test-sentinel"]
timestamp: "2026-09-11T15:09:42.100Z"
x-kage-id: "repo:sparkling-jingling-flute:bug_fix:removing-a-cache-entry-before-refilling-it-leaked-its-footprint-the-budget-count"
x-kage-type: "bug_fix"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["crates/elephc-magician/src/script_cache/store.rs", "crates/elephc-magician/src/script_cache/store_tests.rs"]
x-kage-stack: ["rust"]
---

# Removing a cache entry before refilling it leaked its footprint: the budget counted the same bytes twice

> script cache::compile file removed the existing entry before calling fill entry , to clear the forced invalidate latc…

`script_cache::compile_file()` removed the existing entry before calling `fill_entry`, to clear the forced-invalidate latch. `HashMap::remove` does not return the entry's bytes to `used_memory`, and `fill_entry` computes `replacing` from whether the key is still present — so after the removal it saw `replacing = 0` and added the footprint again. Three compiles of one file charged the budget three times (measured: 33 bytes reported for an 11-byte entry), and repeated recompiles eventually cross `opcache.memory_consumption`, latch `cache_full`, and stop caching a file the cache already held.

The removal was not merely harmful, it was UNNECESSARY: `fill_entry` inserts a fresh `Entry { discarded: false, .. }`, so the refill already clears the latch.

TESTING NOTE worth repeating: the first sentinel written for this (recompiling at `max_accelerated_files = 1`) PASSED against the defect, because the removal frees a slot rather than consuming one — the ceiling is never reached. Only after re-introducing the defect and watching which assertions actually fired was it replaced with the real second consequence, the `cache_full` latch under a tight `memory_consumption`. A sentinel that has not been run against the defect is not a sentinel.
Evidence: With the defect re-introduced, `recompiling_a_file_charges_the_budget_once` failed with left: 33, right: 11 and `repeated_recompiles_never_exhaust_the_budget` failed on the `cache_full` assertion; both pass with the fix, and the full 1240-test magician lib suite is green.
Verified by: cargo test -p elephc-magician --lib, run with the defect present and with it removed.

## Verification

With the defect re-introduced, `recompiling_a_file_charges_the_budget_once` failed with left: 33, right: 11 and `repeated_recompiles_never_exhaust_the_budget` failed on the `cache_full` assertion; both pass with the fix, and the full 1240-test magician lib suite is green.

# Citations

[1] explicit_capture (2026-09-11T15:09:42.100Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:sparkling-jingling-flute:bug_fix:removing-a-cache-entry-before-refilling-it-leaked-its-footprint-the-budget-count","title":"Removing a cache entry before refilling it leaked its footprint: the budget counted the same bytes twice","summary":"script cache::compile file removed the existing entry before calling fill entry , to clear the forced invalidate latch. HashMap::remove does not return the entry's bytes to used memory , and fill entry computes replacing","body":"`script_cache::compile_file()` removed the existing entry before calling `fill_entry`, to clear the forced-invalidate latch. `HashMap::remove` does not return the entry's bytes to `used_memory`, and `fill_entry` computes `replacing` from whether the key is still present — so after the removal it saw `replacing = 0` and added the footprint again. Three compiles of one file charged the budget three times (measured: 33 bytes reported for an 11-byte entry), and repeated recompiles eventually cross `opcache.memory_consumption`, latch `cache_full`, and stop caching a file the cache already held.\n\nThe removal was not merely harmful, it was UNNECESSARY: `fill_entry` inserts a fresh `Entry { discarded: false, .. }`, so the refill already clears the latch.\n\nTESTING NOTE worth repeating: the first sentinel written for this (recompiling at `max_accelerated_files = 1`) PASSED against the defect, because the removal frees a slot rather than consuming one — the ceiling is never reached. Only after re-introducing the defect and watching which assertions actually fired was it replaced with the real second consequence, the `cache_full` latch under a tight `memory_consumption`. A sentinel that has not been run against the defect is not a sentinel.\nEvidence: With the defect re-introduced, `recompiling_a_file_charges_the_budget_once` failed with left: 33, right: 11 and `repeated_recompiles_never_exhaust_the_budget` failed on the `cache_full` assertion; both pass with the fix, and the full 1240-test magician lib suite is green.\nVerified by: cargo test -p elephc-magician --lib, run with the defect present and with it removed.","type":"bug_fix","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","opcache","script-cache","accounting","test-sentinel"],"paths":["crates/elephc-magician/src/script_cache/store.rs","crates/elephc-magician/src/script_cache/store_tests.rs"],"stack":["rust"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-11T15:09:42.100Z"}],"context":{"fact":"`script_cache::compile_file()` removed the existing entry before calling `fill_entry`, to clear the forced-invalidate latch. `HashMap::remove` does not return the entry's bytes to `used_memory`, and `fill_entry` computes `replacing` from whether the key is still present — so after the removal it saw `replacing = 0` and added the footprint again. Three compiles of one file charged the budget three times (measured: 33 bytes reported for an 11-byte entry), and repeated recompiles eventually cross `opcache.memory_consumption`, latch `cache_full`, and stop caching a file the cache already held.","verification":"With the defect re-introduced, `recompiling_a_file_charges_the_budget_once` failed with left: 33, right: 11 and `repeated_recompiles_never_exhaust_the_budget` failed on the `cache_full` assertion; both pass with the fix, and the full 1240-test magician lib suite is green."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-11T15:09:42.100Z","path_fingerprints":[{"path":"crates/elephc-magician/src/script_cache/store.rs","sha256":"55b1b6d750069775aae459ba6ae7266db1c46250abf9cca11abbdf6cee0807d4","size":14452},{"path":"crates/elephc-magician/src/script_cache/store_tests.rs","sha256":"1bd71b541c3d27c5e171557c04acde1b2cc7244fa7023aae5dad861e1bd6d7e9","size":14260}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":8000,"discovery_tokens_estimated":true,"score":100,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","concise but substantive","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"estimated_tokens_saved":408,"stale":true,"stale_reasons":["linked path changed since memory was verified: crates/elephc-magician/src/script_cache/store.rs, crates/elephc-magician/src/script_cache/store_tests.rs"],"suggested_action":"update"},"created_at":"2026-09-11T15:09:42.100Z","updated_at":"2026-09-12T18:12:29.313Z","author_branch":"feat/opcache-runtime-cache"}
```

