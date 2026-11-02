---
type: "Gotcha"
title: "OPEN: driving an interpreter-created Generator from AOT corrupts memory -- the crash point MOVES with unrelated codegen, so chase it as corruption not as a bad dereference"
description: "OPEN, NOT fixed. The last layer under the Symfony web SIGSEGV, and the single most important thing to know before chasing it is that IT IS NOT A FIXED BAD POINTER: the crash point moves when you change unrelated code aro"
resource: "crates/elephc-magician/src/interpreter/generators.rs"
tags: ["session-learning", "symfony-web", "generator", "eval-bridge", "memory-corruption", "iterable", "open-bug"]
timestamp: "2026-09-12T18:56:27.033Z"
x-kage-id: "repo:lazy-petting-popcorn:gotcha:open-driving-an-interpreter-created-generator-from-aot-corrupts-memory-the-crash"
x-kage-type: "gotcha"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["crates/elephc-magician/src/interpreter/generators.rs", "src/codegen/lower_inst/method_dispatch.rs", "src/codegen/eval_method_helpers.rs", "tests/codegen/runtime_gc.rs"]
x-kage-stack: ["rust", "php", "symfony", "macos-aarch64"]
---

# OPEN: driving an interpreter-created Generator from AOT corrupts memory -- the crash point MOVES with unrelated codegen, so chase it as corruption not as a bad dereference

> OPEN, NOT fixed. The last layer under the Symfony web SIGSEGV, and the single most important thing to know before cha…

OPEN, NOT fixed. The last layer under the Symfony `--web` SIGSEGV, and the single most important thing to know before chasing it is that IT IS NOT A FIXED BAD POINTER: the crash point moves when you change unrelated code around it, which means something corrupts memory earlier and the fault merely surfaces wherever the next codegen layout happens to touch it.

THE EVIDENCE FOR THAT, two runs of the same probe differing only in statements AFTER the failing one. An AOT `drive(iterable $g)` guarded by `if (!($g instanceof Iterator)) return;` then walking the protocol:
  version A -- `echo "[valid=", ($g->valid()?'1':'0'), "]"; echo "[cur=", $g->current(), "]";`
      elephc: [inst][valid=1][cur=        <- valid() SUCCEEDED and printed 1, current() died
  version B -- same, but `$c = $g->current();` captured first, then its type printed
      elephc: [valid=                     <- now valid() itself dies, before printing its result
php -n 8.5.10 prints the full line for both versions and for both callers. A logic error in
`current()` cannot make `valid()` start failing; a heap or stack scribble can.

WHAT IS ESTABLISHED AND SHOULD NOT BE RE-DERIVED. The bridge dispatch WORKS: `instanceof Iterator`
answers true and `valid()` returned the correct `1` in version A, so an interpreter-owned generator
IS reachable and drivable through the eval bridge at least one call deep. The ARGUMENT now arrives
correctly too -- a separate fix landed this session made an eval-created Generator passed to an
AOT `iterable` parameter arrive as a Generator instead of an `array` (`get_debug_type` proves it).
So this is neither an argument-marshalling nor a dispatch-routing problem; it is state corruption
during or after the bridged generator call.

WHERE TO POINT THE TOOLS. This repo already has the machinery for exactly this class of bug:
`tests/codegen/runtime_gc.rs` and its module directory, plus the heap-debug leak summary the
runtime can emit (the eval-reflection ownership test in tests/codegen/eval.rs already diffs that
line between a single and a repeated run). Drive the probe under that instead of reading lowering
code, and compare one generator call against two. Suspect the ownership/refcount handling of the
value crossing back from the interpreter: `valid()` returns a bool and `current()` a Mixed, and the
failure appears at or just after the first value-returning bridged call.

A FOURTH `iterable` OMISSION SURFACED WHILE PROBING, unrelated to the corruption but worth fixing:
a method call on an UN-NARROWED `iterable` receiver does not compile at all --
`unsupported EIR backend feature: method call receiver for PHP type Iterable (op method_call)`,
from the `PhpType::Object(class_name)` else-arm in `lower_native_method_call`
(src/codegen/lower_inst/method_dispatch.rs). Real PHP allows `$it->valid()` on an `iterable` and
raises "Call to a member function on array" only for the array half at run time. Symfony's own code
narrows with `instanceof` first, so this does not block the request -- but it is the same pattern as
the other three `iterable` omissions fixed this session, and it is a hard compile failure for
ordinary PHP.
Evidence: scratchpad/red10/steps.php in two versions against php -n 8.5.10, which prints `AOT [inst][valid=1][cur=1][next][cur2=2] done EVAL [inst][valid=1][cur=1][next][cur2=2] done after` and `AOT [valid=1][got][type=int][val=1] done EVAL [valid=1][got][type=int][val=1] done after` respectively. elephc prints the AOT half correctly in BOTH versions and truncates the EVAL half at `[inst][valid=1][cur=` in version A and at `[valid=` in version B -- the same call succeeding in one and failing in the other. The un-narrowed variant fails to compile with `unsupported EIR backend feature: method call receiver for PHP type Iterable (line 10, op method_call)`. Symfony `--web` after the argument fix: unchanged, `worker terminated by signal 11`, crash report index-2026-09-12-205359.ips still KERN_INVALID_ADDRESS 0x0000000100000020 at `__rt_mixed_unbox` / `...ResourceCheckerConfigCache___isfresh`.
Verified by: php -n 8.5.10 against two variants of the same reducer showing a moving crash point, plus the unchanged Symfony crash report

## Verification

scratchpad/red10/steps.php in two versions against php -n 8.5.10, which prints `AOT [inst][valid=1][cur=1][next][cur2=2] done EVAL [inst][valid=1][cur=1][next][cur2=2] done after` and `AOT [valid=1][got][type=int][val=1] done EVAL [valid=1][got][type=int][val=1] done after` respectively. elephc prints the AOT half correctly in BOTH versions and truncates the EVAL half at `[inst][valid=1][cur=` in version A and at `[valid=` in version B -- the same call succeeding in one and failing in the other. The un-narrowed variant fails to compile with `unsupported EIR backend feature: method call receiver for PHP type Iterable (line 10, op method_call)`. Symfony `--web` after the argument fix: unchanged, `worker terminated by signal 11`, crash report index-2026-09-12-205359.ips still KERN_INVALID_ADDRESS 0x0000000100000020 at `__rt_mixed_unbox` / `...ResourceCheckerConfigCache___isfresh`.

# Citations

[1] explicit_capture (2026-09-12T18:56:27.033Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:gotcha:open-driving-an-interpreter-created-generator-from-aot-corrupts-memory-the-crash","title":"OPEN: driving an interpreter-created Generator from AOT corrupts memory -- the crash point MOVES with unrelated codegen, so chase it as corruption not as a bad dereference","summary":"OPEN, NOT fixed. The last layer under the Symfony web SIGSEGV, and the single most important thing to know before chasing it is that IT IS NOT A FIXED BAD POINTER: the crash point moves when you change unrelated code aro","body":"OPEN, NOT fixed. The last layer under the Symfony `--web` SIGSEGV, and the single most important thing to know before chasing it is that IT IS NOT A FIXED BAD POINTER: the crash point moves when you change unrelated code around it, which means something corrupts memory earlier and the fault merely surfaces wherever the next codegen layout happens to touch it.\n\nTHE EVIDENCE FOR THAT, two runs of the same probe differing only in statements AFTER the failing one. An AOT `drive(iterable $g)` guarded by `if (!($g instanceof Iterator)) return;` then walking the protocol:\n  version A -- `echo \"[valid=\", ($g->valid()?'1':'0'), \"]\"; echo \"[cur=\", $g->current(), \"]\";`\n      elephc: [inst][valid=1][cur=        <- valid() SUCCEEDED and printed 1, current() died\n  version B -- same, but `$c = $g->current();` captured first, then its type printed\n      elephc: [valid=                     <- now valid() itself dies, before printing its result\nphp -n 8.5.10 prints the full line for both versions and for both callers. A logic error in\n`current()` cannot make `valid()` start failing; a heap or stack scribble can.\n\nWHAT IS ESTABLISHED AND SHOULD NOT BE RE-DERIVED. The bridge dispatch WORKS: `instanceof Iterator`\nanswers true and `valid()` returned the correct `1` in version A, so an interpreter-owned generator\nIS reachable and drivable through the eval bridge at least one call deep. The ARGUMENT now arrives\ncorrectly too -- a separate fix landed this session made an eval-created Generator passed to an\nAOT `iterable` parameter arrive as a Generator instead of an `array` (`get_debug_type` proves it).\nSo this is neither an argument-marshalling nor a dispatch-routing problem; it is state corruption\nduring or after the bridged generator call.\n\nWHERE TO POINT THE TOOLS. This repo already has the machinery for exactly this class of bug:\n`tests/codegen/runtime_gc.rs` and its module directory, plus the heap-debug leak summary the\nruntime can emit (the eval-reflection ownership test in tests/codegen/eval.rs already diffs that\nline between a single and a repeated run). Drive the probe under that instead of reading lowering\ncode, and compare one generator call against two. Suspect the ownership/refcount handling of the\nvalue crossing back from the interpreter: `valid()` returns a bool and `current()` a Mixed, and the\nfailure appears at or just after the first value-returning bridged call.\n\nA FOURTH `iterable` OMISSION SURFACED WHILE PROBING, unrelated to the corruption but worth fixing:\na method call on an UN-NARROWED `iterable` receiver does not compile at all --\n`unsupported EIR backend feature: method call receiver for PHP type Iterable (op method_call)`,\nfrom the `PhpType::Object(class_name)` else-arm in `lower_native_method_call`\n(src/codegen/lower_inst/method_dispatch.rs). Real PHP allows `$it->valid()` on an `iterable` and\nraises \"Call to a member function on array\" only for the array half at run time. Symfony's own code\nnarrows with `instanceof` first, so this does not block the request -- but it is the same pattern as\nthe other three `iterable` omissions fixed this session, and it is a hard compile failure for\nordinary PHP.\nEvidence: scratchpad/red10/steps.php in two versions against php -n 8.5.10, which prints `AOT [inst][valid=1][cur=1][next][cur2=2] done EVAL [inst][valid=1][cur=1][next][cur2=2] done after` and `AOT [valid=1][got][type=int][val=1] done EVAL [valid=1][got][type=int][val=1] done after` respectively. elephc prints the AOT half correctly in BOTH versions and truncates the EVAL half at `[inst][valid=1][cur=` in version A and at `[valid=` in version B -- the same call succeeding in one and failing in the other. The un-narrowed variant fails to compile with `unsupported EIR backend feature: method call receiver for PHP type Iterable (line 10, op method_call)`. Symfony `--web` after the argument fix: unchanged, `worker terminated by signal 11`, crash report index-2026-09-12-205359.ips still KERN_INVALID_ADDRESS 0x0000000100000020 at `__rt_mixed_unbox` / `...ResourceCheckerConfigCache___isfresh`.\nVerified by: php -n 8.5.10 against two variants of the same reducer showing a moving crash point, plus the unchanged Symfony crash report","type":"gotcha","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","symfony-web","generator","eval-bridge","memory-corruption","iterable","open-bug"],"paths":["crates/elephc-magician/src/interpreter/generators.rs","src/codegen/lower_inst/method_dispatch.rs","src/codegen/eval_method_helpers.rs","tests/codegen/runtime_gc.rs"],"stack":["rust","php","symfony","macos-aarch64"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-12T18:56:27.033Z"}],"context":{"fact":"OPEN, NOT fixed. The last layer under the Symfony `--web` SIGSEGV, and the single most important thing to know before chasing it is that IT IS NOT A FIXED BAD POINTER: the crash point moves when you change unrelated code around it, which means something corrupts memory earlier and the fault merely surfaces wherever the next codegen layout happens to touch it.","verification":"scratchpad/red10/steps.php in two versions against php -n 8.5.10, which prints `AOT [inst][valid=1][cur=1][next][cur2=2] done EVAL [inst][valid=1][cur=1][next][cur2=2] done after` and `AOT [valid=1][got][type=int][val=1] done EVAL [valid=1][got][type=int][val=1] done after` respectively. elephc prints the AOT half correctly in BOTH versions and truncates the EVAL half at `[inst][valid=1][cur=` in version A and at `[valid=` in version B -- the same call succeeding in one and failing in the other. The un-narrowed variant fails to compile with `unsupported EIR backend feature: method call receiver for PHP type Iterable (line 10, op method_call)`. Symfony `--web` after the argument fix: unchanged, `worker terminated by signal 11`, crash report index-2026-09-12-205359.ips still KERN_INVALID_ADDRESS 0x0000000100000020 at `__rt_mixed_unbox` / `...ResourceCheckerConfigCache___isfresh`."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-12T18:56:27.033Z","path_fingerprints":[{"path":"crates/elephc-magician/src/interpreter/generators.rs","sha256":"856f5f3567ad004147f370c85af2f8d377e568a57f552ac35ec76ecce4f0d8dc","size":37150},{"path":"src/codegen/lower_inst/method_dispatch.rs","sha256":"e9b56433ab4773e863eeaa0b310f74170d5b1cd2e590562c054e5a830546e9b0","size":37731},{"path":"src/codegen/eval_method_helpers.rs","sha256":"74041cde9fba2cc86a1cb927e473c9055f9b63e92823b61df042e10f4d6e97a4","size":130840},{"path":"tests/codegen/runtime_gc.rs","sha256":"7213b551e82220d87025352ac76c1cffd01b2e42b2e019647d17ec61ee2b12dd","size":1560}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":8000,"discovery_tokens_estimated":true,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"stale_reasons":[],"estimated_tokens_saved":1050},"created_at":"2026-09-12T18:56:27.033Z","updated_at":"2026-09-12T18:56:27.033Z","author_branch":"reconcile/dirname-symfony"}
```

