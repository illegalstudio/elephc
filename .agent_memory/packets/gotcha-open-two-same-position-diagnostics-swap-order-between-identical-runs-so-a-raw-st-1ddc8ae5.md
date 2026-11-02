---
type: "Gotcha"
title: "OPEN: two same-position diagnostics swap order between identical runs, so a raw-stderr identity gate gives a spurious verdict"
description: "OPEN DEFECT, found 2026 09 05 while establishing the compile time identity gate; NOT fixed in that echelon. Two consecutive runs of the SAME release compiler over the SAME input emit byte identical assembly 34,949,996 B,"
resource: "src/types/checker"
tags: ["session-learning", "nondeterminism", "diagnostics", "identity-gate", "open-bug", "reproducibility"]
timestamp: "2026-09-05T13:04:55.105Z"
x-kage-id: "repo:lazy-petting-popcorn:gotcha:open-two-same-position-diagnostics-swap-order-between-identical-runs-so-a-raw-st"
x-kage-type: "gotcha"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["src/types/checker", "src/cli.rs"]
x-kage-stack: ["rust"]
---

# OPEN: two same-position diagnostics swap order between identical runs, so a raw-stderr identity gate gives a spurious verdict

> OPEN DEFECT, found 2026 09 05 while establishing the compile time identity gate; NOT fixed in that echelon. Two conse…

OPEN DEFECT, found 2026-09-05 while establishing the compile-time identity gate; NOT fixed in that echelon. Two consecutive runs of the SAME release compiler over the SAME input emit byte-identical assembly (34,949,996 B, cmp exit 0) but stderr that differs: `warning[33:5]: Unused variable: $resource` and `warning[33:5]: Unused variable: $timestamp` swap places. Sorting both makes them compare equal, so it is the same multiset in a different order — a HashMap (or other unordered) iteration order is reaching the diagnostic stream for diagnostics that share a source position.

TWO CONSEQUENCES.
1. Product: diagnostic output is not reproducible run to run. Anything that diffs compiler stderr — a test fixture, a CI golden file, a user's build log comparison — can fail for no reason.
2. Method: ANY identity gate in this repo must compare SORTED stderr, not raw stderr. The perf hand-over asserted "stderr identical" for its own two runs; that was luck, not a property. A raw comparison would have reported "DIAGNOSTICS DIFFER" on every patch in the compile-time echelon and sent the executor hunting a nonexistent regression.

The fix belongs where unused-variable diagnostics are collected: give diagnostics sharing a position a deterministic tiebreak (declaration order, or the variable name) before they are emitted. Evidence files kept as the two raw stderr captures of the same input.
Evidence: Two consecutive `elephc --emit-asm examples/symfony-app/freshprobe2.php` runs, release compiler at 283cd2fe4f with frozen bridge archives: `cmp` on the emitted .s exit 0 (byte-identical, 34,949,996 B), `cmp` on stderr exit 1, `diff` showing only lines 11/12 transposed, `cmp` on the two sorted stderr files exit 0.
Verified by: Reproduced twice in the same session on a clean worktree at 283cd2fe4f; the assembly being byte-identical rules out any real difference in what was compiled

## Verification

Two consecutive `elephc --emit-asm examples/symfony-app/freshprobe2.php` runs, release compiler at 283cd2fe4f with frozen bridge archives: `cmp` on the emitted .s exit 0 (byte-identical, 34,949,996 B), `cmp` on stderr exit 1, `diff` showing only lines 11/12 transposed, `cmp` on the two sorted stderr files exit 0.

# Citations

[1] explicit_capture (2026-09-05T13:04:55.105Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:gotcha:open-two-same-position-diagnostics-swap-order-between-identical-runs-so-a-raw-st","title":"OPEN: two same-position diagnostics swap order between identical runs, so a raw-stderr identity gate gives a spurious verdict","summary":"OPEN DEFECT, found 2026 09 05 while establishing the compile time identity gate; NOT fixed in that echelon. Two consecutive runs of the SAME release compiler over the SAME input emit byte identical assembly 34,949,996 B,","body":"OPEN DEFECT, found 2026-09-05 while establishing the compile-time identity gate; NOT fixed in that echelon. Two consecutive runs of the SAME release compiler over the SAME input emit byte-identical assembly (34,949,996 B, cmp exit 0) but stderr that differs: `warning[33:5]: Unused variable: $resource` and `warning[33:5]: Unused variable: $timestamp` swap places. Sorting both makes them compare equal, so it is the same multiset in a different order — a HashMap (or other unordered) iteration order is reaching the diagnostic stream for diagnostics that share a source position.\n\nTWO CONSEQUENCES.\n1. Product: diagnostic output is not reproducible run to run. Anything that diffs compiler stderr — a test fixture, a CI golden file, a user's build log comparison — can fail for no reason.\n2. Method: ANY identity gate in this repo must compare SORTED stderr, not raw stderr. The perf hand-over asserted \"stderr identical\" for its own two runs; that was luck, not a property. A raw comparison would have reported \"DIAGNOSTICS DIFFER\" on every patch in the compile-time echelon and sent the executor hunting a nonexistent regression.\n\nThe fix belongs where unused-variable diagnostics are collected: give diagnostics sharing a position a deterministic tiebreak (declaration order, or the variable name) before they are emitted. Evidence files kept as the two raw stderr captures of the same input.\nEvidence: Two consecutive `elephc --emit-asm examples/symfony-app/freshprobe2.php` runs, release compiler at 283cd2fe4f with frozen bridge archives: `cmp` on the emitted .s exit 0 (byte-identical, 34,949,996 B), `cmp` on stderr exit 1, `diff` showing only lines 11/12 transposed, `cmp` on the two sorted stderr files exit 0.\nVerified by: Reproduced twice in the same session on a clean worktree at 283cd2fe4f; the assembly being byte-identical rules out any real difference in what was compiled","type":"gotcha","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","nondeterminism","diagnostics","identity-gate","open-bug","reproducibility"],"paths":["src/types/checker","src/cli.rs"],"stack":["rust"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-05T13:04:55.105Z"}],"context":{"fact":"OPEN DEFECT, found 2026-09-05 while establishing the compile-time identity gate; NOT fixed in that echelon. Two consecutive runs of the SAME release compiler over the SAME input emit byte-identical assembly (34,949,996 B, cmp exit 0) but stderr that differs: `warning[33:5]: Unused variable: $resource` and `warning[33:5]: Unused variable: $timestamp` swap places. Sorting both makes them compare equal, so it is the same multiset in a different order — a HashMap (or other unordered) iteration order is reaching the diagnostic stream for diagnostics that share a source position.","verification":"Two consecutive `elephc --emit-asm examples/symfony-app/freshprobe2.php` runs, release compiler at 283cd2fe4f with frozen bridge archives: `cmp` on the emitted .s exit 0 (byte-identical, 34,949,996 B), `cmp` on stderr exit 1, `diff` showing only lines 11/12 transposed, `cmp` on the two sorted stderr files exit 0."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-05T13:04:55.105Z","path_fingerprints":[{"path":"src/cli.rs","sha256":"216f2750e1b2fa3c1dd21555f538df6c7165a3e9fc0be5270f6fd95066ffc63b","size":63767}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":25000,"discovery_tokens_estimated":false,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"stale_reasons":[],"estimated_tokens_saved":473},"created_at":"2026-09-05T13:04:55.105Z","updated_at":"2026-09-05T13:04:55.105Z","author_branch":"reconcile/dirname-symfony"}
```

