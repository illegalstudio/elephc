---
type: "Runbook"
title: "NEXT STEP, exact site: the Iterator-protocol driving in iterator_to_array/foreach needs the eval-ownership probe that method calls already have"
description: "This is the remaining half of the Symfony web blocker, located to the function. Method calls on an interpreter created Generator now work the owner context is published and lower method call 's dynamic ownership probe ro"
resource: "src/codegen/lower_inst/builtins/spl.rs"
tags: ["session-learning", "symfony-web", "generator", "eval-bridge", "ownership-probe", "iterator-to-array", "next-step"]
timestamp: "2026-09-12T19:12:04.674Z"
x-kage-id: "repo:lazy-petting-popcorn:runbook:next-step-exact-site-the-iterator-protocol-driving-in-iterator-to-array-foreach-"
x-kage-type: "runbook"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["src/codegen/lower_inst/builtins/spl.rs", "src/codegen/lower_inst/builtins/eval/dynamic_calls.rs", "src/codegen/lower_inst/method_dispatch.rs", "crates/elephc-magician/src/context/generators.rs"]
x-kage-stack: ["rust", "php", "symfony", "macos-aarch64"]
---

# NEXT STEP, exact site: the Iterator-protocol driving in iterator_to_array/foreach needs the eval-ownership probe that method calls already have

> This is the remaining half of the Symfony web blocker, located to the function. Method calls on an interpreter create…

This is the remaining half of the Symfony `--web` blocker, located to the function. Method calls on an
interpreter-created Generator now work (the owner context is published and `lower_method_call`'s
dynamic-ownership probe routes them through the bridge); BUILTIN iteration and `foreach` still do not,
because they drive the Iterator protocol natively with no such probe.

THE PATH, top to bottom:
  `iterator_to_array($x)` -> RuntimeFnId::IteratorToArray
    -> `lower_iterator_to_array` (src/codegen/lower_inst/builtins/spl.rs:286)
    -> `emit_to_array_loaded_source` (same file, ~820), which switches on the operand's codegen type
    -> for `PhpType::Iterable`, `emit_to_array_loaded_iterable` (~1255). That calls `__rt_heap_kind`
       and branches: kind 2 indexed array, kind 3 hash, kind 4 OBJECT -> `object_case`
    -> `emit_to_array_loaded_traversable_object`, which drives rewind/valid/current/next natively.
An interpreter-owned generator reaches `object_case` and is driven natively, which switches onto an
activation stack it never had -- register-level proof in the companion packet: `sp` inside `heap_buf`,
fp = 0x2, lr = 0x4, and `ret` into 0x4.

WHAT TO DO. In the `object_case` branch (and the equivalent in `emit_apply_loaded_iterable` for
`iterator_apply`, and in the `foreach` iteration lowering, which fails the same way with a HANG rather
than a crash), probe `__elephc_eval_object_has_dynamic_owner` on the object pointer first and, when it
answers non-zero, drive the object through the bridge instead of natively. The model to copy is
`lower_eval_owned_method_call` (src/codegen/lower_inst/builtins/eval/dynamic_calls.rs:594): it loads
the receiver, unboxes when the static type is Mixed, calls the probe, branches to a native-fallback
label on zero, and otherwise packs the arguments and calls `__elephc_eval_method_call`. Note the probe
must be given the raw object pointer -- which, for an `iterable` operand, it now is, since the invoker
coercion was fixed this session to unbox `PhpType::Iterable`.

DO NOT try these, each was measured and failed: gating on `builtins::has_eval_context(ctx)` (false in
the AOT callee -- the eval context belongs to the caller, so no static per-function test works);
suspecting the promoted property or the object (reading the property back gives the right value and
`get_debug_type` says `Generator`); and `--heap-debug` (its leak summary prints at exit, which the
crash never reaches).

HOW TO VERIFY, cheapest first. scratchpad/red9 is the shaped probe: an AOT class with a promoted
`private iterable`, built by interpreted code with a two-element generator, whose methods do
`iterator_to_array($this->items)` and `foreach ($this->items ...)` separately. php -n 8.5.10 prints
`AOT read-ok convert:2 foreach:2 / EVAL read:read-ok / EVAL convert:2 / EVAL foreach:2 / after`.
Put ONE consumer per run -- `convert` crashes and `foreach` hangs, and alternating between them
wastes a build. Then rebuild examples/symfony-app with --web and curl it; the target is HTTP 404 with
39,315 bytes containing "Welcome to Symfony!", which is what php -n serves for the same fixture.
Evidence: scratchpad/red9 with `iterator_to_array` ordered first: elephc prints `AOT read-ok convert:2 foreach:2 EVAL read:read-ok EVAL convert:` then dies; with `foreach` first it hangs at `EVAL foreach:` (killed at 300 s). scratchpad/red10 (same generator driven by explicit method calls) now matches php exactly after the owner-context fix, which is what isolates the failure to the iteration path rather than the generator or the property. Call chain read directly in src/codegen/lower_inst/builtins/spl.rs at lines 286, 820, 1255 and the `__rt_heap_kind` kind-4 branch. Symfony --web unchanged: worker terminated by signal 11, crash report index-2026-09-12-211025.ips, `__rt_mixed_unbox` / `...ResourceCheckerConfigCache___isfresh`, whose source line is `iterator_to_array($this->resourceCheckers)`.
Verified by: Reading the lowering chain against the measured behaviour of two reducers, one of which now passes after the owner-context fix and one of which still fails

## Verification

scratchpad/red9 with `iterator_to_array` ordered first: elephc prints `AOT read-ok convert:2 foreach:2 EVAL read:read-ok EVAL convert:` then dies; with `foreach` first it hangs at `EVAL foreach:` (killed at 300 s). scratchpad/red10 (same generator driven by explicit method calls) now matches php exactly after the owner-context fix, which is what isolates the failure to the iteration path rather than the generator or the property. Call chain read directly in src/codegen/lower_inst/builtins/spl.rs at lines 286, 820, 1255 and the `__rt_heap_kind` kind-4 branch. Symfony --web unchanged: worker terminated by signal 11, crash report index-2026-09-12-211025.ips, `__rt_mixed_unbox` / `...ResourceCheckerConfigCache___isfresh`, whose source line is `iterator_to_array($this->resourceCheckers)`.

# Citations

[1] explicit_capture (2026-09-12T19:12:04.674Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:runbook:next-step-exact-site-the-iterator-protocol-driving-in-iterator-to-array-foreach-","title":"NEXT STEP, exact site: the Iterator-protocol driving in iterator_to_array/foreach needs the eval-ownership probe that method calls already have","summary":"This is the remaining half of the Symfony web blocker, located to the function. Method calls on an interpreter created Generator now work the owner context is published and lower method call 's dynamic ownership probe ro","body":"This is the remaining half of the Symfony `--web` blocker, located to the function. Method calls on an\ninterpreter-created Generator now work (the owner context is published and `lower_method_call`'s\ndynamic-ownership probe routes them through the bridge); BUILTIN iteration and `foreach` still do not,\nbecause they drive the Iterator protocol natively with no such probe.\n\nTHE PATH, top to bottom:\n  `iterator_to_array($x)` -> RuntimeFnId::IteratorToArray\n    -> `lower_iterator_to_array` (src/codegen/lower_inst/builtins/spl.rs:286)\n    -> `emit_to_array_loaded_source` (same file, ~820), which switches on the operand's codegen type\n    -> for `PhpType::Iterable`, `emit_to_array_loaded_iterable` (~1255). That calls `__rt_heap_kind`\n       and branches: kind 2 indexed array, kind 3 hash, kind 4 OBJECT -> `object_case`\n    -> `emit_to_array_loaded_traversable_object`, which drives rewind/valid/current/next natively.\nAn interpreter-owned generator reaches `object_case` and is driven natively, which switches onto an\nactivation stack it never had -- register-level proof in the companion packet: `sp` inside `heap_buf`,\nfp = 0x2, lr = 0x4, and `ret` into 0x4.\n\nWHAT TO DO. In the `object_case` branch (and the equivalent in `emit_apply_loaded_iterable` for\n`iterator_apply`, and in the `foreach` iteration lowering, which fails the same way with a HANG rather\nthan a crash), probe `__elephc_eval_object_has_dynamic_owner` on the object pointer first and, when it\nanswers non-zero, drive the object through the bridge instead of natively. The model to copy is\n`lower_eval_owned_method_call` (src/codegen/lower_inst/builtins/eval/dynamic_calls.rs:594): it loads\nthe receiver, unboxes when the static type is Mixed, calls the probe, branches to a native-fallback\nlabel on zero, and otherwise packs the arguments and calls `__elephc_eval_method_call`. Note the probe\nmust be given the raw object pointer -- which, for an `iterable` operand, it now is, since the invoker\ncoercion was fixed this session to unbox `PhpType::Iterable`.\n\nDO NOT try these, each was measured and failed: gating on `builtins::has_eval_context(ctx)` (false in\nthe AOT callee -- the eval context belongs to the caller, so no static per-function test works);\nsuspecting the promoted property or the object (reading the property back gives the right value and\n`get_debug_type` says `Generator`); and `--heap-debug` (its leak summary prints at exit, which the\ncrash never reaches).\n\nHOW TO VERIFY, cheapest first. scratchpad/red9 is the shaped probe: an AOT class with a promoted\n`private iterable`, built by interpreted code with a two-element generator, whose methods do\n`iterator_to_array($this->items)` and `foreach ($this->items ...)` separately. php -n 8.5.10 prints\n`AOT read-ok convert:2 foreach:2 / EVAL read:read-ok / EVAL convert:2 / EVAL foreach:2 / after`.\nPut ONE consumer per run -- `convert` crashes and `foreach` hangs, and alternating between them\nwastes a build. Then rebuild examples/symfony-app with --web and curl it; the target is HTTP 404 with\n39,315 bytes containing \"Welcome to Symfony!\", which is what php -n serves for the same fixture.\nEvidence: scratchpad/red9 with `iterator_to_array` ordered first: elephc prints `AOT read-ok convert:2 foreach:2 EVAL read:read-ok EVAL convert:` then dies; with `foreach` first it hangs at `EVAL foreach:` (killed at 300 s). scratchpad/red10 (same generator driven by explicit method calls) now matches php exactly after the owner-context fix, which is what isolates the failure to the iteration path rather than the generator or the property. Call chain read directly in src/codegen/lower_inst/builtins/spl.rs at lines 286, 820, 1255 and the `__rt_heap_kind` kind-4 branch. Symfony --web unchanged: worker terminated by signal 11, crash report index-2026-09-12-211025.ips, `__rt_mixed_unbox` / `...ResourceCheckerConfigCache___isfresh`, whose source line is `iterator_to_array($this->resourceCheckers)`.\nVerified by: Reading the lowering chain against the measured behaviour of two reducers, one of which now passes after the owner-context fix and one of which still fails","type":"runbook","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","symfony-web","generator","eval-bridge","ownership-probe","iterator-to-array","next-step"],"paths":["src/codegen/lower_inst/builtins/spl.rs","src/codegen/lower_inst/builtins/eval/dynamic_calls.rs","src/codegen/lower_inst/method_dispatch.rs","crates/elephc-magician/src/context/generators.rs"],"stack":["rust","php","symfony","macos-aarch64"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-12T19:12:04.674Z"}],"context":{"fact":"This is the remaining half of the Symfony `--web` blocker, located to the function. Method calls on an\ninterpreter-created Generator now work (the owner context is published and `lower_method_call`'s\ndynamic-ownership probe routes them through the bridge); BUILTIN iteration and `foreach` still do not,\nbecause they drive the Iterator protocol natively with no such probe.","verification":"scratchpad/red9 with `iterator_to_array` ordered first: elephc prints `AOT read-ok convert:2 foreach:2 EVAL read:read-ok EVAL convert:` then dies; with `foreach` first it hangs at `EVAL foreach:` (killed at 300 s). scratchpad/red10 (same generator driven by explicit method calls) now matches php exactly after the owner-context fix, which is what isolates the failure to the iteration path rather than the generator or the property. Call chain read directly in src/codegen/lower_inst/builtins/spl.rs at lines 286, 820, 1255 and the `__rt_heap_kind` kind-4 branch. Symfony --web unchanged: worker terminated by signal 11, crash report index-2026-09-12-211025.ips, `__rt_mixed_unbox` / `...ResourceCheckerConfigCache___isfresh`, whose source line is `iterator_to_array($this->resourceCheckers)`."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-12T19:12:04.674Z","path_fingerprints":[{"path":"src/codegen/lower_inst/builtins/spl.rs","sha256":"50ad3418f7fe1cc1e39dc86a50b54b3f51efda02a4d48363fb83717cbe84efbc","size":95730},{"path":"src/codegen/lower_inst/builtins/eval/dynamic_calls.rs","sha256":"382b05d86227b94ec41189979d0687a03723e36dd30171784f0aea766dafc07c","size":42073},{"path":"src/codegen/lower_inst/method_dispatch.rs","sha256":"e9b56433ab4773e863eeaa0b310f74170d5b1cd2e590562c054e5a830546e9b0","size":37731},{"path":"crates/elephc-magician/src/context/generators.rs","sha256":"2cf23ee5822d399fa53235106268fbad97c8074540bfea4344b15580a192f518","size":8921}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":2000,"discovery_tokens_estimated":true,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"stale_reasons":[],"estimated_tokens_saved":1028},"created_at":"2026-09-12T19:12:04.674Z","updated_at":"2026-09-12T19:12:04.674Z","author_branch":"reconcile/dirname-symfony"}
```

