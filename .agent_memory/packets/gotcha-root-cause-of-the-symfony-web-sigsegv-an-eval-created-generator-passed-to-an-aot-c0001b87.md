---
type: "Gotcha"
title: "ROOT CAUSE of the Symfony --web SIGSEGV: an eval-created Generator passed to an AOT `iterable` parameter arrives as an ARRAY"
description: "OPEN, NOT fixed, but reduced to a WRONG VALUE with no crash which makes it the cleanest possible statement of the bug that blocks the Symfony web request. THE REDUCER, 20 lines, pure AOT function plus one runtime include"
resource: "src/codegen/runtime_callable_invoker.rs"
tags: ["session-learning", "symfony-web", "iterable", "generator", "eval-bridge", "abi", "root-cause", "open-bug"]
timestamp: "2026-09-12T18:26:05.306Z"
x-kage-id: "repo:lazy-petting-popcorn:gotcha:root-cause-of-the-symfony-web-sigsegv-an-eval-created-generator-passed-to-an-aot"
x-kage-type: "gotcha"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["src/codegen/runtime_callable_invoker.rs", "src/codegen/lower_inst/builtins/eval/function_registration.rs", "src/codegen/eval_method_helpers.rs", "src/ir_lower/gradual_coercions.rs", "src/types/model.rs"]
x-kage-stack: ["rust", "php", "symfony", "macos-aarch64"]
---

# ROOT CAUSE of the Symfony --web SIGSEGV: an eval-created Generator passed to an AOT `iterable` parameter arrives as an ARRAY

> OPEN, NOT fixed, but reduced to a WRONG VALUE with no crash which makes it the cleanest possible statement of the bug…

OPEN, NOT fixed, but reduced to a WRONG VALUE with no crash -- which makes it the cleanest possible statement of the bug that blocks the Symfony `--web` request.

THE REDUCER, 20 lines, pure AOT function plus one runtime-included file:
    function what(iterable $v): string { return get_debug_type($v); }
    // AOT caller
    echo "AOT gen=", what(aot_gen()), " arr=", what([1, 2]);
    // included (interpreted) file
    echo "EVAL gen=", what(eval_gen()), " arr=", what([1, 2]);
  php -n 8.5.10 : AOT gen=Generator arr=array   EVAL gen=Generator arr=array
  elephc        : AOT gen=Generator arr=array   EVAL gen=array     arr=array
                                                     ^^^^^
An eval-created Generator handed to an AOT function's `iterable` parameter ARRIVES AS AN ARRAY.
No crash, no diagnostic -- just the wrong kind. The array case is right, and the AOT-created
generator is right; only the eval-created object loses its object-ness crossing the boundary.

EVERY DOWNSTREAM SYMPTOM FALLS OUT OF THIS ONE FACT, which is why chasing them individually kept
dead-ending: `get_debug_type` answers "array"; `instanceof Traversable/Iterator` is handed an array
pointer, so it answers false or -- once the iterable-instanceof fix stops folding it to a constant
-- dereferences it and CRASHES; `iterator_to_array()` segfaults; `foreach` spins forever. Symfony's
`ResourceCheckerConfigCache::isFresh()` is exactly this shape: the generated container feeds its
promoted `private iterable $resourceCheckers` a RewindableGenerator, and the compiled `isFresh`
body then dies in `__rt_mixed_unbox` at KERN_INVALID_ADDRESS 0x0000000100000020.

WHERE IT LIVES. An AOT function is exposed to the interpreter by
`register_eval_native_function` (src/codegen/lower_inst/builtins/eval/function_registration.rs),
whose invoker is emitted by `runtime_callable_invoker.rs`. That invoker groups `PhpType::Iterable`
WITH `Mixed | Union(_)` when it marshals a cell -- see line ~1637, which stamps runtime tag 7
(the gradual/boxed tag) where an array would get 4/5 and an object 6. But an `iterable` ABI slot
does NOT hold a boxed cell: coercing a gradual value to `iterable` emits `Op::MixedUnbox`
(src/ir_lower/gradual_coercions.rs:104) and `src/types/model.rs:120` calls it a "type-erased
pointer (array|Traversable)". So the two sides disagree about the representation, and the object
half is the half that breaks. Note the METHOD and CONSTRUCTOR bridges get this RIGHT: they have a
dedicated `emit_*_cast_eval_iterable_arg` (src/codegen/eval_method_helpers.rs) that unboxes, checks
the tag for 4/5/6 and validates the object against the Traversable interface ids. The function
path has no equivalent. The fix is to give the invoker that same treatment -- but it is a change to
the ABI contract of EVERY callable descriptor, so decide deliberately whether an `iterable` slot
carries the boxed cell or the unboxed payload, and make both sides agree.

MEASURED DEAD ENDS, do not repeat: routing Generator intrinsics to the bridge on
`has_eval_context(ctx)` (false in the AOT callee -- the eval context belongs to the CALLER);
publishing the generator's owner context in `register_eval_generator` (the registration runs,
traced, and the crash is unchanged because execution dies before the ownership probe); and
suspecting the promoted property or the object (both incidental -- the plain function argument
above reproduces it with neither).
Evidence: scratchpad/red10/arg.php + arggen.php: php -n 8.5.10 prints `AOT gen=Generator arr=array / EVAL gen=Generator arr=array / after`; elephc prints `AOT gen=Generator arr=array / EVAL gen=array arr=array / after`, exit 0 -- a wrong value, not a crash. scratchpad/red10/main.php (the same boundary plus the Iterator protocol): elephc gives the AOT line correctly and exits 138 on the eval line. scratchpad/red9: through a promoted `iterable` property, `iterator_to_array` exits 139 and `foreach` hangs (killed at 300 s) where php prints `convert:2 foreach:2`. Crash reports main-2026-09-12-195752.ips and index-2026-09-12-195528.ips share KERN_INVALID_ADDRESS 0x0000000100000020, the Symfony one symbolicated as `__rt_mixed_unbox` / `...ResourceCheckerConfigCache___isfresh`. Negative controls: arrays cross the same boundary correctly, and an AOT-created generator is correct in every probe. Regression slices with the session's fixes in place: generator 96/0, exception 145/2, instanceof 59/2, iterable 120/13 -- all 17 failures reproduce on a pristine tree.
Verified by: php -n 8.5.10 against a 20-line pure-AOT reducer that shows the wrong kind without crashing, plus four narrowing controls and the shared crash address with the Symfony worker

## Verification

scratchpad/red10/arg.php + arggen.php: php -n 8.5.10 prints `AOT gen=Generator arr=array / EVAL gen=Generator arr=array / after`; elephc prints `AOT gen=Generator arr=array / EVAL gen=array arr=array / after`, exit 0 -- a wrong value, not a crash. scratchpad/red10/main.php (the same boundary plus the Iterator protocol): elephc gives the AOT line correctly and exits 138 on the eval line. scratchpad/red9: through a promoted `iterable` property, `iterator_to_array` exits 139 and `foreach` hangs (killed at 300 s) where php prints `convert:2 foreach:2`. Crash reports main-2026-09-12-195752.ips and index-2026-09-12-195528.ips share KERN_INVALID_ADDRESS 0x0000000100000020, the Symfony one symbolicated as `__rt_mixed_unbox` / `...ResourceCheckerConfigCache___isfresh`. Negative controls: arrays cross the same boundary correctly, and an AOT-created generator is correct in every probe. Regression slices with the session's fixes in place: generator 96/0, exception 145/2, instanceof 59/2, iterable 120/13 -- all 17 failures reproduce on a pristine tree.

# Citations

[1] explicit_capture (2026-09-12T18:26:05.306Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:gotcha:root-cause-of-the-symfony-web-sigsegv-an-eval-created-generator-passed-to-an-aot","title":"ROOT CAUSE of the Symfony --web SIGSEGV: an eval-created Generator passed to an AOT `iterable` parameter arrives as an ARRAY","summary":"OPEN, NOT fixed, but reduced to a WRONG VALUE with no crash which makes it the cleanest possible statement of the bug that blocks the Symfony web request. THE REDUCER, 20 lines, pure AOT function plus one runtime include","body":"OPEN, NOT fixed, but reduced to a WRONG VALUE with no crash -- which makes it the cleanest possible statement of the bug that blocks the Symfony `--web` request.\n\nTHE REDUCER, 20 lines, pure AOT function plus one runtime-included file:\n    function what(iterable $v): string { return get_debug_type($v); }\n    // AOT caller\n    echo \"AOT gen=\", what(aot_gen()), \" arr=\", what([1, 2]);\n    // included (interpreted) file\n    echo \"EVAL gen=\", what(eval_gen()), \" arr=\", what([1, 2]);\n  php -n 8.5.10 : AOT gen=Generator arr=array   EVAL gen=Generator arr=array\n  elephc        : AOT gen=Generator arr=array   EVAL gen=array     arr=array\n                                                     ^^^^^\nAn eval-created Generator handed to an AOT function's `iterable` parameter ARRIVES AS AN ARRAY.\nNo crash, no diagnostic -- just the wrong kind. The array case is right, and the AOT-created\ngenerator is right; only the eval-created object loses its object-ness crossing the boundary.\n\nEVERY DOWNSTREAM SYMPTOM FALLS OUT OF THIS ONE FACT, which is why chasing them individually kept\ndead-ending: `get_debug_type` answers \"array\"; `instanceof Traversable/Iterator` is handed an array\npointer, so it answers false or -- once the iterable-instanceof fix stops folding it to a constant\n-- dereferences it and CRASHES; `iterator_to_array()` segfaults; `foreach` spins forever. Symfony's\n`ResourceCheckerConfigCache::isFresh()` is exactly this shape: the generated container feeds its\npromoted `private iterable $resourceCheckers` a RewindableGenerator, and the compiled `isFresh`\nbody then dies in `__rt_mixed_unbox` at KERN_INVALID_ADDRESS 0x0000000100000020.\n\nWHERE IT LIVES. An AOT function is exposed to the interpreter by\n`register_eval_native_function` (src/codegen/lower_inst/builtins/eval/function_registration.rs),\nwhose invoker is emitted by `runtime_callable_invoker.rs`. That invoker groups `PhpType::Iterable`\nWITH `Mixed | Union(_)` when it marshals a cell -- see line ~1637, which stamps runtime tag 7\n(the gradual/boxed tag) where an array would get 4/5 and an object 6. But an `iterable` ABI slot\ndoes NOT hold a boxed cell: coercing a gradual value to `iterable` emits `Op::MixedUnbox`\n(src/ir_lower/gradual_coercions.rs:104) and `src/types/model.rs:120` calls it a \"type-erased\npointer (array|Traversable)\". So the two sides disagree about the representation, and the object\nhalf is the half that breaks. Note the METHOD and CONSTRUCTOR bridges get this RIGHT: they have a\ndedicated `emit_*_cast_eval_iterable_arg` (src/codegen/eval_method_helpers.rs) that unboxes, checks\nthe tag for 4/5/6 and validates the object against the Traversable interface ids. The function\npath has no equivalent. The fix is to give the invoker that same treatment -- but it is a change to\nthe ABI contract of EVERY callable descriptor, so decide deliberately whether an `iterable` slot\ncarries the boxed cell or the unboxed payload, and make both sides agree.\n\nMEASURED DEAD ENDS, do not repeat: routing Generator intrinsics to the bridge on\n`has_eval_context(ctx)` (false in the AOT callee -- the eval context belongs to the CALLER);\npublishing the generator's owner context in `register_eval_generator` (the registration runs,\ntraced, and the crash is unchanged because execution dies before the ownership probe); and\nsuspecting the promoted property or the object (both incidental -- the plain function argument\nabove reproduces it with neither).\nEvidence: scratchpad/red10/arg.php + arggen.php: php -n 8.5.10 prints `AOT gen=Generator arr=array / EVAL gen=Generator arr=array / after`; elephc prints `AOT gen=Generator arr=array / EVAL gen=array arr=array / after`, exit 0 -- a wrong value, not a crash. scratchpad/red10/main.php (the same boundary plus the Iterator protocol): elephc gives the AOT line correctly and exits 138 on the eval line. scratchpad/red9: through a promoted `iterable` property, `iterator_to_array` exits 139 and `foreach` hangs (killed at 300 s) where php prints `convert:2 foreach:2`. Crash reports main-2026-09-12-195752.ips and index-2026-09-12-195528.ips share KERN_INVALID_ADDRESS 0x0000000100000020, the Symfony one symbolicated as `__rt_mixed_unbox` / `...ResourceCheckerConfigCache___isfresh`. Negative controls: arrays cross the same boundary correctly, and an AOT-created generator is correct in every probe. Regression slices with the session's fixes in place: generator 96/0, exception 145/2, instanceof 59/2, iterable 120/13 -- all 17 failures reproduce on a pristine tree.\nVerified by: php -n 8.5.10 against a 20-line pure-AOT reducer that shows the wrong kind without crashing, plus four narrowing controls and the shared crash address with the Symfony worker","type":"gotcha","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","symfony-web","iterable","generator","eval-bridge","abi","root-cause","open-bug"],"paths":["src/codegen/runtime_callable_invoker.rs","src/codegen/lower_inst/builtins/eval/function_registration.rs","src/codegen/eval_method_helpers.rs","src/ir_lower/gradual_coercions.rs","src/types/model.rs"],"stack":["rust","php","symfony","macos-aarch64"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-12T18:26:05.306Z"}],"context":{"fact":"OPEN, NOT fixed, but reduced to a WRONG VALUE with no crash -- which makes it the cleanest possible statement of the bug that blocks the Symfony `--web` request.","verification":"scratchpad/red10/arg.php + arggen.php: php -n 8.5.10 prints `AOT gen=Generator arr=array / EVAL gen=Generator arr=array / after`; elephc prints `AOT gen=Generator arr=array / EVAL gen=array arr=array / after`, exit 0 -- a wrong value, not a crash. scratchpad/red10/main.php (the same boundary plus the Iterator protocol): elephc gives the AOT line correctly and exits 138 on the eval line. scratchpad/red9: through a promoted `iterable` property, `iterator_to_array` exits 139 and `foreach` hangs (killed at 300 s) where php prints `convert:2 foreach:2`. Crash reports main-2026-09-12-195752.ips and index-2026-09-12-195528.ips share KERN_INVALID_ADDRESS 0x0000000100000020, the Symfony one symbolicated as `__rt_mixed_unbox` / `...ResourceCheckerConfigCache___isfresh`. Negative controls: arrays cross the same boundary correctly, and an AOT-created generator is correct in every probe. Regression slices with the session's fixes in place: generator 96/0, exception 145/2, instanceof 59/2, iterable 120/13 -- all 17 failures reproduce on a pristine tree."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-12T18:26:05.306Z","path_fingerprints":[{"path":"src/codegen/runtime_callable_invoker.rs","sha256":"0da1d749b6a4213ad9852d5696d3db6afbc559dae5d0f00d01550d97d63a3428","size":136336},{"path":"src/codegen/lower_inst/builtins/eval/function_registration.rs","sha256":"8f894cd84533121051bc241423cae48efcc8d95a8291a407cacad609a17fb8fe","size":6050},{"path":"src/codegen/eval_method_helpers.rs","sha256":"74041cde9fba2cc86a1cb927e473c9055f9b63e92823b61df042e10f4d6e97a4","size":130840},{"path":"src/ir_lower/gradual_coercions.rs","sha256":"c6f9d05fb9c27cac452f3f3fcf6a4515665f3acbaa9bbc241d11cbf751753b7f","size":5765},{"path":"src/types/model.rs","sha256":"3b375d1544a4838d7ecfc6dfb8f6e7d927d5c1a4daa827ddbac8519149c836a1","size":13695}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":8000,"discovery_tokens_estimated":true,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"stale_reasons":[],"estimated_tokens_saved":1173},"created_at":"2026-09-12T18:26:05.306Z","updated_at":"2026-09-12T18:26:05.306Z","author_branch":"reconcile/dirname-symfony"}
```

