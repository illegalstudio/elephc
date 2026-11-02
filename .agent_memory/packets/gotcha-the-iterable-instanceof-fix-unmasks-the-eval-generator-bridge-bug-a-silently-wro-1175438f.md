---
type: "Gotcha"
title: "The iterable-instanceof fix UNMASKS the eval-generator bridge bug: a silently wrong false becomes a crash, and the two must land together"
description: "READ THIS BEFORE JUDGING EITHER FIX ON ITS OWN. Two defects sit on top of each other, and fixing the outer one makes the inner one visible as a crash. Measured on one 25 line pure AOT probe scratchpad/red10 that drives a"
resource: "src/codegen/lower_inst/objects/instanceof_entry.rs"
tags: ["session-learning", "instanceof", "iterable", "generator", "eval-bridge", "symfony-web", "unmasking"]
timestamp: "2026-09-12T18:24:07.527Z"
x-kage-id: "repo:lazy-petting-popcorn:gotcha:the-iterable-instanceof-fix-unmasks-the-eval-generator-bridge-bug-a-silently-wro"
x-kage-type: "gotcha"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["src/codegen/lower_inst/objects/instanceof_entry.rs", "src/codegen/lower_inst/method_dispatch.rs", "crates/elephc-magician/src/context/generators.rs", "src/codegen/lower_inst/builtins/eval/dynamic_calls.rs"]
x-kage-stack: ["rust", "php", "macos-aarch64"]
---

# The iterable-instanceof fix UNMASKS the eval-generator bridge bug: a silently wrong false becomes a crash, and the two must land together

> READ THIS BEFORE JUDGING EITHER FIX ON ITS OWN. Two defects sit on top of each other, and fixing the outer one makes …

READ THIS BEFORE JUDGING EITHER FIX ON ITS OWN. Two defects sit on top of each other, and fixing the outer one makes the inner one visible as a crash. Measured on one 25-line pure-AOT probe (scratchpad/red10) that drives a generator through `valid()` / `current()` / `next()` inside `drive(iterable $g)`:

  WITHOUT the iterable-instanceof fix:
    AOT-declared generator : "not-iterator"   <- SILENTLY WRONG (php: valid:1 current:1 then:2)
    eval-declared generator: "not-iterator"   <- silently wrong, but no crash
  WITH it:
    AOT-declared generator : valid:1 current:1 then:2   <- now CORRECT, matches php
    eval-declared generator: CRASH (exit 138)

The old behaviour was not a conservative refusal, it was a false answer: `$g instanceof Iterator`
answered no for an object that IS an Iterator, so the guard sent BOTH cases down the
"not-iterator" path and nothing ever reached the generator. Fixing `instanceof` makes the AOT case
right and lets the eval case actually reach the generator -- where the real, pre-existing defect
lives: an interpreter-created Generator has no AOT activation, so the compiled Iterator protocol
dereferences garbage.

CONSEQUENCE FOR SHIPPING. The instanceof fix is correct and stays, but it converts a silent wrong
answer into a loud crash for any program that drives an EVAL-created generator from AOT code. It
does not make Symfony worse -- that request crashed either way, only one branch earlier -- but it
is a behaviour change worth landing together with the generator-bridge fix, or at least stating in
the same changelog entry. Expect other dead branches to wake up the same way: this fix already
forced `local load from PHP type Iterable as Object(..)` to be handled in
`local_load_types_share_storage`, a pairing that had never been reachable.

WHAT THE OWNERSHIP PROBE DOES AND DOES NOT CATCH, so the next attempt starts past this.
`lower_method_call` (src/codegen/lower_inst/method_dispatch.rs) already emits a runtime
dynamic-ownership probe before native dispatch -- `__elephc_eval_object_has_dynamic_owner`, then
`__elephc_eval_method_call` on a hit -- and `--emit-asm` confirms both calls ARE emitted inside the
failing function. It still misses the eval generator. Registering the generator's owner context in
`register_eval_generator` (the `register_dynamic_object_context` half only, NOT
`register_dynamic_object`, since `Generator` is not an eval-declared class) was tried: the
registration demonstrably runs -- traced as `phase=generator_register identity=<n>` -- and the
crash is UNCHANGED, and the trace shows no `method_dispatch` phase at all, so execution never
reaches the probe. It dies earlier, in the `instanceof` matcher itself. That attempt was reverted,
partly because it also put a global mutex on every generator creation.

TWO MORE DEAD ENDS, measured: routing Generator intrinsics to the bridge whenever
`has_eval_context(ctx)` changes nothing, because that flag is false in the AOT callee -- the eval
context belongs to the CALLER, so no static per-function test can decide this. And the receiver at
the failing call site is narrowed to `Object("Iterator")`, not `Generator`, so
`generator_intrinsic(class, method)` does not even fire there; the call goes through interface
dispatch.
Evidence: scratchpad/red10 with and without the instanceof fix, against `php -n` 8.5.10 which prints `AOT valid:1 current:1 then:2` and `EVAL valid:1 current:1 then:2`: pristine tree gives `AOT not-iterator / EVAL not-iterator / after` exit 0; with the fix, `AOT valid:1 current:1 then:2` then the eval case exits 138. scratchpad/red7/builtin.php and arrays.php re-verified after reapplying: `obj:yy iter:yy mixed:yy` and `empty:nn list:nn assoc:nn nested:nn obj:yy again:nnyy`, both identical to php. `--emit-asm` on red10 shows `bl ___elephc_eval_object_has_dynamic_owner` and `bl ___elephc_eval_method_call` inside `_fn_drive`, after a `bl __rt_exception_matches`. The registration experiment traced `phase=generator_register identity=4310233088` with no subsequent dispatch phase before the crash. Regression slices with the fix in place: generator 96/0, exception 145/2, instanceof 59/2, iterable 120/13 -- all 17 failures reproduce on a pristine tree.
Verified by: php -n 8.5.10 against the same reducer on the pristine tree and with the fix, --emit-asm of the failing function, and an instrumented registration experiment that was reverted

## Verification

scratchpad/red10 with and without the instanceof fix, against `php -n` 8.5.10 which prints `AOT valid:1 current:1 then:2` and `EVAL valid:1 current:1 then:2`: pristine tree gives `AOT not-iterator / EVAL not-iterator / after` exit 0; with the fix, `AOT valid:1 current:1 then:2` then the eval case exits 138. scratchpad/red7/builtin.php and arrays.php re-verified after reapplying: `obj:yy iter:yy mixed:yy` and `empty:nn list:nn assoc:nn nested:nn obj:yy again:nnyy`, both identical to php. `--emit-asm` on red10 shows `bl ___elephc_eval_object_has_dynamic_owner` and `bl ___elephc_eval_method_call` inside `_fn_drive`, after a `bl __rt_exception_matches`. The registration experiment traced `phase=generator_register identity=4310233088` with no subsequent dispatch phase before the crash. Regression slices with the fix in place: generator 96/0, exception 145/2, instanceof 59/2, iterable 120/13 -- all 17 failures reproduce on a pristine tree.

# Citations

[1] explicit_capture (2026-09-12T18:24:07.527Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:gotcha:the-iterable-instanceof-fix-unmasks-the-eval-generator-bridge-bug-a-silently-wro","title":"The iterable-instanceof fix UNMASKS the eval-generator bridge bug: a silently wrong false becomes a crash, and the two must land together","summary":"READ THIS BEFORE JUDGING EITHER FIX ON ITS OWN. Two defects sit on top of each other, and fixing the outer one makes the inner one visible as a crash. Measured on one 25 line pure AOT probe scratchpad/red10 that drives a","body":"READ THIS BEFORE JUDGING EITHER FIX ON ITS OWN. Two defects sit on top of each other, and fixing the outer one makes the inner one visible as a crash. Measured on one 25-line pure-AOT probe (scratchpad/red10) that drives a generator through `valid()` / `current()` / `next()` inside `drive(iterable $g)`:\n\n  WITHOUT the iterable-instanceof fix:\n    AOT-declared generator : \"not-iterator\"   <- SILENTLY WRONG (php: valid:1 current:1 then:2)\n    eval-declared generator: \"not-iterator\"   <- silently wrong, but no crash\n  WITH it:\n    AOT-declared generator : valid:1 current:1 then:2   <- now CORRECT, matches php\n    eval-declared generator: CRASH (exit 138)\n\nThe old behaviour was not a conservative refusal, it was a false answer: `$g instanceof Iterator`\nanswered no for an object that IS an Iterator, so the guard sent BOTH cases down the\n\"not-iterator\" path and nothing ever reached the generator. Fixing `instanceof` makes the AOT case\nright and lets the eval case actually reach the generator -- where the real, pre-existing defect\nlives: an interpreter-created Generator has no AOT activation, so the compiled Iterator protocol\ndereferences garbage.\n\nCONSEQUENCE FOR SHIPPING. The instanceof fix is correct and stays, but it converts a silent wrong\nanswer into a loud crash for any program that drives an EVAL-created generator from AOT code. It\ndoes not make Symfony worse -- that request crashed either way, only one branch earlier -- but it\nis a behaviour change worth landing together with the generator-bridge fix, or at least stating in\nthe same changelog entry. Expect other dead branches to wake up the same way: this fix already\nforced `local load from PHP type Iterable as Object(..)` to be handled in\n`local_load_types_share_storage`, a pairing that had never been reachable.\n\nWHAT THE OWNERSHIP PROBE DOES AND DOES NOT CATCH, so the next attempt starts past this.\n`lower_method_call` (src/codegen/lower_inst/method_dispatch.rs) already emits a runtime\ndynamic-ownership probe before native dispatch -- `__elephc_eval_object_has_dynamic_owner`, then\n`__elephc_eval_method_call` on a hit -- and `--emit-asm` confirms both calls ARE emitted inside the\nfailing function. It still misses the eval generator. Registering the generator's owner context in\n`register_eval_generator` (the `register_dynamic_object_context` half only, NOT\n`register_dynamic_object`, since `Generator` is not an eval-declared class) was tried: the\nregistration demonstrably runs -- traced as `phase=generator_register identity=<n>` -- and the\ncrash is UNCHANGED, and the trace shows no `method_dispatch` phase at all, so execution never\nreaches the probe. It dies earlier, in the `instanceof` matcher itself. That attempt was reverted,\npartly because it also put a global mutex on every generator creation.\n\nTWO MORE DEAD ENDS, measured: routing Generator intrinsics to the bridge whenever\n`has_eval_context(ctx)` changes nothing, because that flag is false in the AOT callee -- the eval\ncontext belongs to the CALLER, so no static per-function test can decide this. And the receiver at\nthe failing call site is narrowed to `Object(\"Iterator\")`, not `Generator`, so\n`generator_intrinsic(class, method)` does not even fire there; the call goes through interface\ndispatch.\nEvidence: scratchpad/red10 with and without the instanceof fix, against `php -n` 8.5.10 which prints `AOT valid:1 current:1 then:2` and `EVAL valid:1 current:1 then:2`: pristine tree gives `AOT not-iterator / EVAL not-iterator / after` exit 0; with the fix, `AOT valid:1 current:1 then:2` then the eval case exits 138. scratchpad/red7/builtin.php and arrays.php re-verified after reapplying: `obj:yy iter:yy mixed:yy` and `empty:nn list:nn assoc:nn nested:nn obj:yy again:nnyy`, both identical to php. `--emit-asm` on red10 shows `bl ___elephc_eval_object_has_dynamic_owner` and `bl ___elephc_eval_method_call` inside `_fn_drive`, after a `bl __rt_exception_matches`. The registration experiment traced `phase=generator_register identity=4310233088` with no subsequent dispatch phase before the crash. Regression slices with the fix in place: generator 96/0, exception 145/2, instanceof 59/2, iterable 120/13 -- all 17 failures reproduce on a pristine tree.\nVerified by: php -n 8.5.10 against the same reducer on the pristine tree and with the fix, --emit-asm of the failing function, and an instrumented registration experiment that was reverted","type":"gotcha","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","instanceof","iterable","generator","eval-bridge","symfony-web","unmasking"],"paths":["src/codegen/lower_inst/objects/instanceof_entry.rs","src/codegen/lower_inst/method_dispatch.rs","crates/elephc-magician/src/context/generators.rs","src/codegen/lower_inst/builtins/eval/dynamic_calls.rs"],"stack":["rust","php","macos-aarch64"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-12T18:24:07.527Z"}],"context":{"fact":"READ THIS BEFORE JUDGING EITHER FIX ON ITS OWN. Two defects sit on top of each other, and fixing the outer one makes the inner one visible as a crash. Measured on one 25-line pure-AOT probe (scratchpad/red10) that drives a generator through `valid()` / `current()` / `next()` inside `drive(iterable $g)`:","verification":"scratchpad/red10 with and without the instanceof fix, against `php -n` 8.5.10 which prints `AOT valid:1 current:1 then:2` and `EVAL valid:1 current:1 then:2`: pristine tree gives `AOT not-iterator / EVAL not-iterator / after` exit 0; with the fix, `AOT valid:1 current:1 then:2` then the eval case exits 138. scratchpad/red7/builtin.php and arrays.php re-verified after reapplying: `obj:yy iter:yy mixed:yy` and `empty:nn list:nn assoc:nn nested:nn obj:yy again:nnyy`, both identical to php. `--emit-asm` on red10 shows `bl ___elephc_eval_object_has_dynamic_owner` and `bl ___elephc_eval_method_call` inside `_fn_drive`, after a `bl __rt_exception_matches`. The registration experiment traced `phase=generator_register identity=4310233088` with no subsequent dispatch phase before the crash. Regression slices with the fix in place: generator 96/0, exception 145/2, instanceof 59/2, iterable 120/13 -- all 17 failures reproduce on a pristine tree."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-12T18:24:07.527Z","path_fingerprints":[{"path":"src/codegen/lower_inst/objects/instanceof_entry.rs","sha256":"5e1e41e19e382818ca48bd374c70997c64cb17aa48f16894bc5fd09f6183e1d2","size":5800},{"path":"src/codegen/lower_inst/method_dispatch.rs","sha256":"e9b56433ab4773e863eeaa0b310f74170d5b1cd2e590562c054e5a830546e9b0","size":37731},{"path":"crates/elephc-magician/src/context/generators.rs","sha256":"de1e8170db53a396cd17c997ff6407af7a7e3961146478a5e5e7f94f55bc9a17","size":7938},{"path":"src/codegen/lower_inst/builtins/eval/dynamic_calls.rs","sha256":"382b05d86227b94ec41189979d0687a03723e36dd30171784f0aea766dafc07c","size":42073}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":8000,"discovery_tokens_estimated":true,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"stale_reasons":[],"estimated_tokens_saved":1103},"created_at":"2026-09-12T18:24:07.527Z","updated_at":"2026-09-12T18:24:07.527Z","author_branch":"reconcile/dirname-symfony"}
```

