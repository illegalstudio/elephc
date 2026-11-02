---
type: "Bug Fix"
title: "Publishing an eval generator's owner context makes AOT method calls on it work; the remaining gap is that builtins and foreach iterate without the ownership probe"
description: "PARTIAL FIX LANDED, and the remaining half is now exactly located. WHAT WAS FIXED. register eval generator crates/elephc magician/src/context/generators.rs now also publishes the generator's owner context via crate::ffi:"
resource: "crates/elephc-magician/src/context/generators.rs"
tags: ["session-learning", "symfony-web", "generator", "eval-bridge", "ownership-probe", "iterable"]
timestamp: "2026-09-12T19:11:09.076Z"
x-kage-id: "repo:lazy-petting-popcorn:bug_fix:publishing-an-eval-generators-owner-context-makes-aot-method-calls-on-it-work-th"
x-kage-type: "bug_fix"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["crates/elephc-magician/src/context/generators.rs", "src/codegen/lower_inst/method_dispatch.rs", "src/codegen/runtime_callable_invoker.rs", "crates/elephc-magician/src/ffi/dynamic_destructors.rs"]
x-kage-stack: ["rust", "php", "symfony", "macos-aarch64"]
---

# Publishing an eval generator's owner context makes AOT method calls on it work; the remaining gap is that builtins and foreach iterate without the ownership probe

> PARTIAL FIX LANDED, and the remaining half is now exactly located. WHAT WAS FIXED. register eval generator crates/ele…

PARTIAL FIX LANDED, and the remaining half is now exactly located.

WHAT WAS FIXED. `register_eval_generator` (crates/elephc-magician/src/context/generators.rs) now also
publishes the generator's owner context via
`crate::ffi::dynamic_destructors::register_dynamic_object_context(identity, self)`. `Generator` is an
AOT class, so a generator the INTERPRETER created is indistinguishable from an AOT one at an AOT call
site: `lower_method_call`'s dynamic-ownership probe (`__elephc_eval_object_has_dynamic_owner`, which
IS emitted -- confirmed with --emit-asm) found no owner, fell through to the native Iterator protocol,
and that switches onto an activation stack this generator never had. Deliberately NOT
`register_dynamic_object`, which would additionally record the object as an instance of an
eval-DECLARED class, and `Generator` is not one.

RESULT: an AOT function driving an interpreter-created generator through `instanceof Iterator`,
`valid()`, `current()`, `next()` now matches `php -n` 8.5.10 exactly, where before it crashed.

THE TWO FIXES ONLY WORK COMPOSED, and this is why the same registration was tried earlier in the
session and abandoned as ineffective: at that point an eval-created Generator passed to an AOT
`iterable` parameter still arrived as an ARRAY (the invoker did not unbox `PhpType::Iterable`), so the
ownership probe was handed the wrong pointer and never matched. Fixing the invoker coercion first and
the registration second is what makes either one observable. If you bisect these two changes
independently, each looks useless.

WHAT IS STILL BROKEN, and it is the Symfony blocker. Method calls take the ownership probe; BUILTINS
and `foreach` do not. `iterator_to_array($this->items)` and `foreach ($this->items as ...)` in AOT code
still drive an interpreter-owned generator natively and still crash or hang -- so Symfony's
`ResourceCheckerConfigCache::isFresh()`, whose operation is exactly
`iterator_to_array($this->resourceCheckers)`, is unchanged: `worker terminated by signal 11`,
KERN_INVALID_ADDRESS 0x0000000100000020 at `__rt_mixed_unbox` / `...isfresh`. The property is NOT the
problem -- reading it back gives the right value and `get_debug_type` says `Generator`; the problem is
that the iteration path has no equivalent of the method-call ownership probe.

A COST TO REVISIT BEFORE SHIPPING: `register_dynamic_object_context` takes a global mutex, and this
change puts that on EVERY generator creation. It is correct but not free; a cheaper per-context
registry, or registering lazily on first AOT hand-off, would avoid the lock on a hot path.
Evidence: scratchpad/red10/steps.php and main.php: before, elephc truncated the EVAL line at `[valid=` or `[inst][valid=1][cur=`; after, both print exactly what `php -n` 8.5.10 prints -- `AOT [valid=1][got][type=int][val=1] done EVAL [valid=1][got][type=int][val=1] done after` and `AOT valid:1 current:1 then:2 EVAL valid:1 current:1 then:2 after`. scratchpad/red9 (the property + builtin path) still fails: `AOT read-ok convert:2 foreach:2 EVAL read:read-ok EVAL convert:` then dies on `iterator_to_array`, and hangs instead when `foreach` runs first, against php's `EVAL read:read-ok EVAL convert:2 EVAL foreach:2`. `cargo test --test codegen_tests generator -- --test-threads=4` = 96 passed / 0 failed with the change. Symfony --web rebuilt and re-requested: unchanged, crash report index-2026-09-12-211025.ips identical to before.
Verified by: php -n 8.5.10 against the method-protocol and property-builtin reducers before and after, plus the generator codegen slice and a fresh Symfony compile-and-curl

## Verification

scratchpad/red10/steps.php and main.php: before, elephc truncated the EVAL line at `[valid=` or `[inst][valid=1][cur=`; after, both print exactly what `php -n` 8.5.10 prints -- `AOT [valid=1][got][type=int][val=1] done EVAL [valid=1][got][type=int][val=1] done after` and `AOT valid:1 current:1 then:2 EVAL valid:1 current:1 then:2 after`. scratchpad/red9 (the property + builtin path) still fails: `AOT read-ok convert:2 foreach:2 EVAL read:read-ok EVAL convert:` then dies on `iterator_to_array`, and hangs instead when `foreach` runs first, against php's `EVAL read:read-ok EVAL convert:2 EVAL foreach:2`. `cargo test --test codegen_tests generator -- --test-threads=4` = 96 passed / 0 failed with the change. Symfony --web rebuilt and re-requested: unchanged, crash report index-2026-09-12-211025.ips identical to before.

# Citations

[1] explicit_capture (2026-09-12T19:11:09.076Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:bug_fix:publishing-an-eval-generators-owner-context-makes-aot-method-calls-on-it-work-th","title":"Publishing an eval generator's owner context makes AOT method calls on it work; the remaining gap is that builtins and foreach iterate without the ownership probe","summary":"PARTIAL FIX LANDED, and the remaining half is now exactly located. WHAT WAS FIXED. register eval generator crates/elephc magician/src/context/generators.rs now also publishes the generator's owner context via crate::ffi:","body":"PARTIAL FIX LANDED, and the remaining half is now exactly located.\n\nWHAT WAS FIXED. `register_eval_generator` (crates/elephc-magician/src/context/generators.rs) now also\npublishes the generator's owner context via\n`crate::ffi::dynamic_destructors::register_dynamic_object_context(identity, self)`. `Generator` is an\nAOT class, so a generator the INTERPRETER created is indistinguishable from an AOT one at an AOT call\nsite: `lower_method_call`'s dynamic-ownership probe (`__elephc_eval_object_has_dynamic_owner`, which\nIS emitted -- confirmed with --emit-asm) found no owner, fell through to the native Iterator protocol,\nand that switches onto an activation stack this generator never had. Deliberately NOT\n`register_dynamic_object`, which would additionally record the object as an instance of an\neval-DECLARED class, and `Generator` is not one.\n\nRESULT: an AOT function driving an interpreter-created generator through `instanceof Iterator`,\n`valid()`, `current()`, `next()` now matches `php -n` 8.5.10 exactly, where before it crashed.\n\nTHE TWO FIXES ONLY WORK COMPOSED, and this is why the same registration was tried earlier in the\nsession and abandoned as ineffective: at that point an eval-created Generator passed to an AOT\n`iterable` parameter still arrived as an ARRAY (the invoker did not unbox `PhpType::Iterable`), so the\nownership probe was handed the wrong pointer and never matched. Fixing the invoker coercion first and\nthe registration second is what makes either one observable. If you bisect these two changes\nindependently, each looks useless.\n\nWHAT IS STILL BROKEN, and it is the Symfony blocker. Method calls take the ownership probe; BUILTINS\nand `foreach` do not. `iterator_to_array($this->items)` and `foreach ($this->items as ...)` in AOT code\nstill drive an interpreter-owned generator natively and still crash or hang -- so Symfony's\n`ResourceCheckerConfigCache::isFresh()`, whose operation is exactly\n`iterator_to_array($this->resourceCheckers)`, is unchanged: `worker terminated by signal 11`,\nKERN_INVALID_ADDRESS 0x0000000100000020 at `__rt_mixed_unbox` / `...isfresh`. The property is NOT the\nproblem -- reading it back gives the right value and `get_debug_type` says `Generator`; the problem is\nthat the iteration path has no equivalent of the method-call ownership probe.\n\nA COST TO REVISIT BEFORE SHIPPING: `register_dynamic_object_context` takes a global mutex, and this\nchange puts that on EVERY generator creation. It is correct but not free; a cheaper per-context\nregistry, or registering lazily on first AOT hand-off, would avoid the lock on a hot path.\nEvidence: scratchpad/red10/steps.php and main.php: before, elephc truncated the EVAL line at `[valid=` or `[inst][valid=1][cur=`; after, both print exactly what `php -n` 8.5.10 prints -- `AOT [valid=1][got][type=int][val=1] done EVAL [valid=1][got][type=int][val=1] done after` and `AOT valid:1 current:1 then:2 EVAL valid:1 current:1 then:2 after`. scratchpad/red9 (the property + builtin path) still fails: `AOT read-ok convert:2 foreach:2 EVAL read:read-ok EVAL convert:` then dies on `iterator_to_array`, and hangs instead when `foreach` runs first, against php's `EVAL read:read-ok EVAL convert:2 EVAL foreach:2`. `cargo test --test codegen_tests generator -- --test-threads=4` = 96 passed / 0 failed with the change. Symfony --web rebuilt and re-requested: unchanged, crash report index-2026-09-12-211025.ips identical to before.\nVerified by: php -n 8.5.10 against the method-protocol and property-builtin reducers before and after, plus the generator codegen slice and a fresh Symfony compile-and-curl","type":"bug_fix","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","symfony-web","generator","eval-bridge","ownership-probe","iterable"],"paths":["crates/elephc-magician/src/context/generators.rs","src/codegen/lower_inst/method_dispatch.rs","src/codegen/runtime_callable_invoker.rs","crates/elephc-magician/src/ffi/dynamic_destructors.rs"],"stack":["rust","php","symfony","macos-aarch64"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-12T19:11:09.076Z"}],"context":{"fact":"PARTIAL FIX LANDED, and the remaining half is now exactly located.","verification":"scratchpad/red10/steps.php and main.php: before, elephc truncated the EVAL line at `[valid=` or `[inst][valid=1][cur=`; after, both print exactly what `php -n` 8.5.10 prints -- `AOT [valid=1][got][type=int][val=1] done EVAL [valid=1][got][type=int][val=1] done after` and `AOT valid:1 current:1 then:2 EVAL valid:1 current:1 then:2 after`. scratchpad/red9 (the property + builtin path) still fails: `AOT read-ok convert:2 foreach:2 EVAL read:read-ok EVAL convert:` then dies on `iterator_to_array`, and hangs instead when `foreach` runs first, against php's `EVAL read:read-ok EVAL convert:2 EVAL foreach:2`. `cargo test --test codegen_tests generator -- --test-threads=4` = 96 passed / 0 failed with the change. Symfony --web rebuilt and re-requested: unchanged, crash report index-2026-09-12-211025.ips identical to before."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-12T19:11:09.076Z","path_fingerprints":[{"path":"crates/elephc-magician/src/context/generators.rs","sha256":"2cf23ee5822d399fa53235106268fbad97c8074540bfea4344b15580a192f518","size":8921},{"path":"src/codegen/lower_inst/method_dispatch.rs","sha256":"e9b56433ab4773e863eeaa0b310f74170d5b1cd2e590562c054e5a830546e9b0","size":37731},{"path":"src/codegen/runtime_callable_invoker.rs","sha256":"bbeab1c688425b052f7cc332d2014c8596ba99036073c7fc0bc012d787873d4d","size":137420},{"path":"crates/elephc-magician/src/ffi/dynamic_destructors.rs","sha256":"2f9611756f68671066025d02545454f9fbc3d33c2932edb5305dc719b34ccfbe","size":7213}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":8000,"discovery_tokens_estimated":true,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"stale_reasons":[],"estimated_tokens_saved":902},"created_at":"2026-09-12T19:11:09.076Z","updated_at":"2026-09-12T19:11:09.076Z","author_branch":"reconcile/dirname-symfony"}
```

