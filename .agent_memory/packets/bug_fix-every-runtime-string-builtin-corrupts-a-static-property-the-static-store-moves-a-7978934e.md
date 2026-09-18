---
type: "Bug Fix"
title: "Every runtime string builtin corrupts a static property: the static store moves a scratch pointer where every other store persists"
description: "A string returned by a runtime helper and stored into a STATIC property keeps a pointer into the shared concat scratch buffer. The next scratch user overwrites the bytes, and the property reads back garbage at the RIGHT"
resource: "src/ir_lower/stmt/static_property_writes.rs"
tags: ["session-learning", "static-property", "ownership", "string", "silent-corruption", "open-bug"]
timestamp: "2026-09-11T17:36:37.874Z"
x-kage-id: "repo:sparkling-jingling-flute:bug_fix:every-runtime-string-builtin-corrupts-a-static-property-the-static-store-moves-a"
x-kage-type: "bug_fix"
x-kage-status: "superseded"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "superseded"
x-kage-paths: ["src/ir_lower/stmt/static_property_writes.rs", "src/ir_lower/context.rs", "src/codegen/lower_inst/static_properties.rs", "src/codegen/lower_inst/ownership.rs"]
x-kage-stack: ["rust", "php", "aarch64"]
---

# Every runtime string builtin corrupts a static property: the static store moves a scratch pointer where every other store persists

> A string returned by a runtime helper and stored into a STATIC property keeps a pointer into the shared concat scratc…

A string returned by a runtime helper and stored into a STATIC property keeps a pointer into the shared concat scratch buffer. The next scratch user overwrites the bytes, and the property reads back garbage at the RIGHT LENGTH — silently. Four-line reproducer:

```php
class B { public static string $s = ""; }
B::$s = strtoupper("second");
$noise = str_repeat("x", 16);
echo B::$s;        // "xxxxxx", php says "SECOND"; strlen() says 6 in both
```

BLAST RADIUS, measured one probe per case:
- Corrupts: `strtoupper`, `strtolower`, `strrev`, `addslashes`, `stripslashes`, `base64_encode`, `bin2hex`, `urlencode`, `rawurlencode`, `urldecode`, `quotemeta`, `nl2br` — 12 of 12 tested, i.e. the whole `RuntimeCallTarget::UnaryString` family.
- Correct: the same value stored into a LOCAL, an INSTANCE property, or an ARRAY element; and `substr()` into a static property.

MECHANISM, read off the emitted assembly side by side:
- instance property (`op=prop_set`): `bl __rt_strtoupper` … `bl __rt_str_persist` … store. ✓
- static property (`op=store_static_property`): `bl __rt_strtoupper` … `bl __rt_heap_free_safe` (on the OLD value) … `str x1, [x9]`. NO persist. ✗

`Op::Acquire` on a `PhpType::Str` lowers to exactly `__rt_str_persist` (`codegen/lower_inst/ownership.rs:27`), and the local store calls `acquire_if_refcounted` UNCONDITIONALLY then releases the source. `lower_static_property_assign` instead moves the value in whenever `value_is_owning_temporary` says it is owned.

🔴 DEAD ENDS, so the next session does not repeat them — NEITHER of these changed the emitted code:
1. Flipping `RuntimeCallTarget::UnaryString(_)` from `true` to `false` in `value_is_owning_builtin_temporary` (`ir_lower/context.rs`). Dead: the tail `matches!` of `value_is_owning_temporary` lists `Op::RuntimeCall` outright, so the arm falls through and the answer is `true` anyway.
2. Adding a `Str` + `Op::RuntimeCall` → not-owning rule just before that tail. Also no effect on the emitted assembly, which means the static store's acquire is NOT gated by `value_is_owning_temporary` at all. The next step is to INSTRUMENT `lower_static_property_assign` (an `eprintln!` of both predicates) rather than reason about it — the reasoning was wrong twice.

Note the codebase already knows the hazard: `finalize_value_ownership_metadata` says "String results stay conservative because `Owned` cannot distinguish heap strings from concat scratch storage". That caveat was never applied to `value_is_owning_temporary`.
Evidence: Twelve probes compiled and diffed against `php -n` 8.5.6, all DIVERGING; the same builtins stored into a local, an instance property or an array element all IDENTICAL. Emitted assembly for the static and instance stores compared directly.
Verified by: Reproduced on macOS arm64 with the branch compiler. The fix is NOT landed: two attempts had no effect and were reverted, and the machine ran out of disk before instrumentation could run.

## Verification

Twelve probes compiled and diffed against `php -n` 8.5.6, all DIVERGING; the same builtins stored into a local, an instance property or an array element all IDENTICAL. Emitted assembly for the static and instance stores compared directly.

# Citations

[1] explicit_capture (2026-09-11T17:36:37.874Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:sparkling-jingling-flute:bug_fix:every-runtime-string-builtin-corrupts-a-static-property-the-static-store-moves-a","title":"Every runtime string builtin corrupts a static property: the static store moves a scratch pointer where every other store persists","summary":"A string returned by a runtime helper and stored into a STATIC property keeps a pointer into the shared concat scratch buffer. The next scratch user overwrites the bytes, and the property reads back garbage at the RIGHT","body":"A string returned by a runtime helper and stored into a STATIC property keeps a pointer into the shared concat scratch buffer. The next scratch user overwrites the bytes, and the property reads back garbage at the RIGHT LENGTH — silently. Four-line reproducer:\n\n```php\nclass B { public static string $s = \"\"; }\nB::$s = strtoupper(\"second\");\n$noise = str_repeat(\"x\", 16);\necho B::$s;        // \"xxxxxx\", php says \"SECOND\"; strlen() says 6 in both\n```\n\nBLAST RADIUS, measured one probe per case:\n- Corrupts: `strtoupper`, `strtolower`, `strrev`, `addslashes`, `stripslashes`, `base64_encode`, `bin2hex`, `urlencode`, `rawurlencode`, `urldecode`, `quotemeta`, `nl2br` — 12 of 12 tested, i.e. the whole `RuntimeCallTarget::UnaryString` family.\n- Correct: the same value stored into a LOCAL, an INSTANCE property, or an ARRAY element; and `substr()` into a static property.\n\nMECHANISM, read off the emitted assembly side by side:\n- instance property (`op=prop_set`): `bl __rt_strtoupper` … `bl __rt_str_persist` … store. ✓\n- static property (`op=store_static_property`): `bl __rt_strtoupper` … `bl __rt_heap_free_safe` (on the OLD value) … `str x1, [x9]`. NO persist. ✗\n\n`Op::Acquire` on a `PhpType::Str` lowers to exactly `__rt_str_persist` (`codegen/lower_inst/ownership.rs:27`), and the local store calls `acquire_if_refcounted` UNCONDITIONALLY then releases the source. `lower_static_property_assign` instead moves the value in whenever `value_is_owning_temporary` says it is owned.\n\n🔴 DEAD ENDS, so the next session does not repeat them — NEITHER of these changed the emitted code:\n1. Flipping `RuntimeCallTarget::UnaryString(_)` from `true` to `false` in `value_is_owning_builtin_temporary` (`ir_lower/context.rs`). Dead: the tail `matches!` of `value_is_owning_temporary` lists `Op::RuntimeCall` outright, so the arm falls through and the answer is `true` anyway.\n2. Adding a `Str` + `Op::RuntimeCall` → not-owning rule just before that tail. Also no effect on the emitted assembly, which means the static store's acquire is NOT gated by `value_is_owning_temporary` at all. The next step is to INSTRUMENT `lower_static_property_assign` (an `eprintln!` of both predicates) rather than reason about it — the reasoning was wrong twice.\n\nNote the codebase already knows the hazard: `finalize_value_ownership_metadata` says \"String results stay conservative because `Owned` cannot distinguish heap strings from concat scratch storage\". That caveat was never applied to `value_is_owning_temporary`.\nEvidence: Twelve probes compiled and diffed against `php -n` 8.5.6, all DIVERGING; the same builtins stored into a local, an instance property or an array element all IDENTICAL. Emitted assembly for the static and instance stores compared directly.\nVerified by: Reproduced on macOS arm64 with the branch compiler. The fix is NOT landed: two attempts had no effect and were reverted, and the machine ran out of disk before instrumentation could run.","type":"bug_fix","scope":"repo","visibility":"team","sensitivity":"internal","status":"superseded","confidence":0.7,"tags":["session-learning","static-property","ownership","string","silent-corruption","open-bug"],"paths":["src/ir_lower/stmt/static_property_writes.rs","src/ir_lower/context.rs","src/codegen/lower_inst/static_properties.rs","src/codegen/lower_inst/ownership.rs"],"stack":["rust","php","aarch64"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-11T17:36:37.874Z"}],"context":{"fact":"A string returned by a runtime helper and stored into a STATIC property keeps a pointer into the shared concat scratch buffer. The next scratch user overwrites the bytes, and the property reads back garbage at the RIGHT LENGTH — silently. Four-line reproducer:","verification":"Twelve probes compiled and diffed against `php -n` 8.5.6, all DIVERGING; the same builtins stored into a local, an instance property or an array element all IDENTICAL. Emitted assembly for the static and instance stores compared directly."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-11T17:36:37.874Z","path_fingerprints":[{"path":"src/ir_lower/stmt/static_property_writes.rs","sha256":"f0a566005ebd10d1cc3c73e337862179f1a6310af6a9b8eead3ea28e2dcee5fa","size":8155},{"path":"src/ir_lower/context.rs","sha256":"b21d4f21ed1c4c85cbb05288ac00b32fa9e9d4443b981f2b45b423e036980cb4","size":155758},{"path":"src/codegen/lower_inst/static_properties.rs","sha256":"8f8e78147e5ebe80899023f17e4d46c4c20cf1002c6ca8bd217e7ccfce3b58aa","size":46250},{"path":"src/codegen/lower_inst/ownership.rs","sha256":"1d33224b786ee1f7fa02e10b04f7974a62127b425d301d892db5505d37b165f6","size":9801}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture","superseded_at":"2026-09-12T16:40:24.754Z","superseded_by":"repo:sparkling-jingling-flute:bug_fix:the-static-property-string-corruption-needed-both-halves-two-predicates-independ","superseded_reason":"The open-bug packet describes the defect as live and records two attempted fixes as having \"no effect\". Commit 831c3cb0ab fixed it, and the replacement packet explicitly corrects that framing: each half had no effect ALONE, and both together are load-bearing. Keeping the old one recallable would hand a future session a fixed bug tagged open-bug."},"edges":[{"relation":"superseded_by","to":"repo:sparkling-jingling-flute:bug_fix:the-static-property-string-corruption-needed-both-halves-two-predicates-independ","evidence":"The open-bug packet describes the defect as live and records two attempted fixes as having \"no effect\". Commit 831c3cb0ab fixed it, and the replacement packet explicitly corrects that framing: each half had no effect ALONE, and both together are load-bearing. Keeping the old one recallable would hand a future session a fixed bug tagged open-bug.","created_at":"2026-09-12T16:40:24.754Z"}],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":8000,"discovery_tokens_estimated":true,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"stale_reasons":[],"estimated_tokens_saved":736,"superseded_by":"repo:sparkling-jingling-flute:bug_fix:the-static-property-string-corruption-needed-both-halves-two-predicates-independ","superseded_reason":"The open-bug packet describes the defect as live and records two attempted fixes as having \"no effect\". Commit 831c3cb0ab fixed it, and the replacement packet explicitly corrects that framing: each half had no effect ALONE, and both together are load-bearing. Keeping the old one recallable would hand a future session a fixed bug tagged open-bug."},"created_at":"2026-09-11T17:36:37.874Z","updated_at":"2026-09-12T16:40:24.754Z","author_branch":"feat/opcache-runtime-cache"}
```

