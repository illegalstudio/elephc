---
type: "Bug Fix"
title: "The static-property string corruption needed BOTH halves: two predicates independently claim ownership"
description: "FIXED commit \"persist a runtime helper string stored into a static property\" . This corrects an earlier packet that recorded the two attempts as having \"no effect\" — each had no effect ALONE, because value is owning temp"
resource: "src/ir_lower/context.rs"
tags: ["session-learning", "static-property", "ownership", "string", "silent-corruption", "test-sentinel"]
timestamp: "2026-09-11T18:30:59.456Z"
x-kage-id: "repo:sparkling-jingling-flute:bug_fix:the-static-property-string-corruption-needed-both-halves-two-predicates-independ"
x-kage-type: "bug_fix"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["src/ir_lower/context.rs", "tests/static_property_string_ownership_tests.rs", "src/ir_lower/stmt/static_property_writes.rs"]
x-kage-stack: ["rust", "php"]
---

# The static-property string corruption needed BOTH halves: two predicates independently claim ownership

> FIXED commit "persist a runtime helper string stored into a static property" . This corrects an earlier packet that r…

FIXED (commit "persist a runtime-helper string stored into a static property"). This corrects an earlier packet that recorded the two attempts as having "no effect" — each had no effect ALONE, because `value_is_owning_temporary` (`src/ir_lower/context.rs`) claims ownership through TWO independent paths and either one alone keeps the old answer:

1. `value_is_owning_builtin_temporary`, whose `RuntimeCallTarget::UnaryString(_) => true` arm short-circuits early.
2. The tail `matches!` of `value_is_owning_temporary`, which lists `Op::RuntimeCall` outright.

Both must stop claiming ownership for a `Str`. Verified by removing each half separately and re-running the regression tests: half 1 alone → 2 of 3 tests FAIL; half 2 alone → the same 2 FAIL; both → all pass. That is the "N halves need N sentinels" rule paying off — a single-half fix looked plausible and was worthless.

🔴 MEASUREMENT TRAP that wasted a cycle: a python edit that "removed" half 2 by slicing between two anchors actually DUPLICATED an unrelated block, because the end anchor appeared BEFORE the start anchor in the file. The test then passed and briefly looked like evidence that half 2 was unnecessary. Always `git diff --stat` after a scripted surgical edit — a 40-line insertion where a deletion was intended is visible there in one line.

Also confirmed while fixing: `Acquire` on a `PhpType::Str` lowers to `__rt_str_persist` (`codegen/lower_inst/ownership.rs:27`), which is why making the value non-owning is the whole fix — the storing consumer's existing `acquire_if_refcounted` then does the copy. Local, instance-property and array-element stores never had the bug because they acquire unconditionally.
Evidence: tests/static_property_string_ownership_tests.rs: 3 tests over the whole UnaryString family plus the three storage locations that were already correct. Each half reverted in isolation puts 2 of the 3 back to failing. Regression suites after the fix: 1687 compiler lib, 292 runtime_gc, 388 string codegen, all green.
Verified by: cargo test --test static_property_string_ownership_tests, -p elephc --lib, --test codegen_tests runtime_gc::, --test codegen_tests strings::

## Verification

tests/static_property_string_ownership_tests.rs: 3 tests over the whole UnaryString family plus the three storage locations that were already correct. Each half reverted in isolation puts 2 of the 3 back to failing. Regression suites after the fix: 1687 compiler lib, 292 runtime_gc, 388 string codegen, all green.

# Citations

[1] explicit_capture (2026-09-11T18:30:59.456Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:sparkling-jingling-flute:bug_fix:the-static-property-string-corruption-needed-both-halves-two-predicates-independ","title":"The static-property string corruption needed BOTH halves: two predicates independently claim ownership","summary":"FIXED commit \"persist a runtime helper string stored into a static property\" . This corrects an earlier packet that recorded the two attempts as having \"no effect\" — each had no effect ALONE, because value is owning temp","body":"FIXED (commit \"persist a runtime-helper string stored into a static property\"). This corrects an earlier packet that recorded the two attempts as having \"no effect\" — each had no effect ALONE, because `value_is_owning_temporary` (`src/ir_lower/context.rs`) claims ownership through TWO independent paths and either one alone keeps the old answer:\n\n1. `value_is_owning_builtin_temporary`, whose `RuntimeCallTarget::UnaryString(_) => true` arm short-circuits early.\n2. The tail `matches!` of `value_is_owning_temporary`, which lists `Op::RuntimeCall` outright.\n\nBoth must stop claiming ownership for a `Str`. Verified by removing each half separately and re-running the regression tests: half 1 alone → 2 of 3 tests FAIL; half 2 alone → the same 2 FAIL; both → all pass. That is the \"N halves need N sentinels\" rule paying off — a single-half fix looked plausible and was worthless.\n\n🔴 MEASUREMENT TRAP that wasted a cycle: a python edit that \"removed\" half 2 by slicing between two anchors actually DUPLICATED an unrelated block, because the end anchor appeared BEFORE the start anchor in the file. The test then passed and briefly looked like evidence that half 2 was unnecessary. Always `git diff --stat` after a scripted surgical edit — a 40-line insertion where a deletion was intended is visible there in one line.\n\nAlso confirmed while fixing: `Acquire` on a `PhpType::Str` lowers to `__rt_str_persist` (`codegen/lower_inst/ownership.rs:27`), which is why making the value non-owning is the whole fix — the storing consumer's existing `acquire_if_refcounted` then does the copy. Local, instance-property and array-element stores never had the bug because they acquire unconditionally.\nEvidence: tests/static_property_string_ownership_tests.rs: 3 tests over the whole UnaryString family plus the three storage locations that were already correct. Each half reverted in isolation puts 2 of the 3 back to failing. Regression suites after the fix: 1687 compiler lib, 292 runtime_gc, 388 string codegen, all green.\nVerified by: cargo test --test static_property_string_ownership_tests, -p elephc --lib, --test codegen_tests runtime_gc::, --test codegen_tests strings::","type":"bug_fix","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","static-property","ownership","string","silent-corruption","test-sentinel"],"paths":["src/ir_lower/context.rs","tests/static_property_string_ownership_tests.rs","src/ir_lower/stmt/static_property_writes.rs"],"stack":["rust","php"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-11T18:30:59.456Z"}],"context":{"fact":"FIXED (commit \"persist a runtime-helper string stored into a static property\"). This corrects an earlier packet that recorded the two attempts as having \"no effect\" — each had no effect ALONE, because `value_is_owning_temporary` (`src/ir_lower/context.rs`) claims ownership through TWO independent paths and either one alone keeps the old answer:","verification":"tests/static_property_string_ownership_tests.rs: 3 tests over the whole UnaryString family plus the three storage locations that were already correct. Each half reverted in isolation puts 2 of the 3 back to failing. Regression suites after the fix: 1687 compiler lib, 292 runtime_gc, 388 string codegen, all green."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-11T18:30:59.456Z","path_fingerprints":[{"path":"src/ir_lower/context.rs","sha256":"c0f3a708acda79756a119743ea8757e4b37a5991afb29268386b8b3b1ab630af","size":157547},{"path":"tests/static_property_string_ownership_tests.rs","sha256":"80f85fa203feba1329da6cbcd8ce4760f6138c25448c8a74b2075b8f7566e74a","size":6765},{"path":"src/ir_lower/stmt/static_property_writes.rs","sha256":"f0a566005ebd10d1cc3c73e337862179f1a6310af6a9b8eead3ea28e2dcee5fa","size":8155}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[{"relation":"supersedes","to":"repo:sparkling-jingling-flute:bug_fix:every-runtime-string-builtin-corrupts-a-static-property-the-static-store-moves-a","evidence":"The open-bug packet describes the defect as live and records two attempted fixes as having \"no effect\". Commit 831c3cb0ab fixed it, and the replacement packet explicitly corrects that framing: each half had no effect ALONE, and both together are load-bearing. Keeping the old one recallable would hand a future session a fixed bug tagged open-bug.","created_at":"2026-09-12T16:40:24.754Z"}],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":8000,"discovery_tokens_estimated":true,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"estimated_tokens_saved":543},"created_at":"2026-09-11T18:30:59.456Z","updated_at":"2026-09-12T16:41:20.949Z","author_branch":"feat/opcache-runtime-cache"}
```

