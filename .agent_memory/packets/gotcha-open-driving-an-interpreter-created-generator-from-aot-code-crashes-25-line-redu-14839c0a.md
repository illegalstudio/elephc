---
type: "Gotcha"
title: "OPEN: driving an INTERPRETER-created Generator from AOT code crashes; 25-line reducer with no object and no property, and it is the Symfony --web SIGSEGV"
description: "OPEN, measured and NOT fixed. This is the root of the Symfony web SIGSEGV, reduced as far as it goes: no object, no property, no web. THE REDUCER, 25 lines. An AOT function drives a generator through the Iterator protoco"
resource: "src/types/checker/builtin_iterators.rs"
tags: ["session-learning", "symfony-web", "generator", "eval-bridge", "segfault", "aot-boundary", "open-bug"]
timestamp: "2026-09-12T18:13:31.252Z"
x-kage-id: "repo:lazy-petting-popcorn:gotcha:open-driving-an-interpreter-created-generator-from-aot-code-crashes-25-line-redu"
x-kage-type: "gotcha"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["src/types/checker/builtin_iterators.rs", "src/codegen/lower_inst/local_loads.rs", "crates/elephc-magician/src/interpreter/generators.rs", "examples/symfony-app/vendor/symfony/config/ResourceCheckerConfigCache.php"]
x-kage-stack: ["rust", "php", "symfony", "macos-aarch64"]
---

# OPEN: driving an INTERPRETER-created Generator from AOT code crashes; 25-line reducer with no object and no property, and it is the Symfony --web SIGSEGV

> OPEN, measured and NOT fixed. This is the root of the Symfony web SIGSEGV, reduced as far as it goes: no object, no p…

OPEN, measured and NOT fixed. This is the root of the Symfony `--web` SIGSEGV, reduced as far as it goes: no object, no property, no --web.

THE REDUCER, 25 lines. An AOT function drives a generator through the Iterator protocol:
    function drive(iterable $g): string {
        if (!($g instanceof Iterator)) { return 'not-iterator'; }
        $out = 'valid:' . ($g->valid() ? '1' : '0');
        $out .= ' current:' . $g->current();
        $g->next();
        return $out . ' then:' . $g->current();
    }
Called with a generator whose function is declared in AOT code: correct, `valid:1 current:1 then:2`,
matching `php -n` 8.5.10. Called with a generator whose function is declared in a runtime-included
(interpreted) file: CRASH, exit 138, where php prints the same line. So AOT `Generator::valid` /
`current` / `next` assume an AOT generator activation, and an interpreter-owned generator has none.

THIS EXPLAINS THE WHOLE CHAIN. Symfony's `ResourceCheckerConfigCache::isFresh()` calls
`iterator_to_array($this->resourceCheckers)`, and the container feeds that property a
`RewindableGenerator` built by interpreted code; the compiled `isFresh` body then drives it and
dies in `__rt_mixed_unbox` at KERN_INVALID_ADDRESS 0x0000000100000020. The earlier packet framing
this as "a promoted `iterable` property on an eval-constructed object" was one layer too high --
the property and the object are incidental. What matters is only WHO CREATED THE GENERATOR.

PROGRESSIVELY NARROWED, so do not redo these: `iterator_to_array()` on an eval-declared generator
called from INTERPRETED code works (prints 2, like php). The same AOT class holding an ARRAY in
its promoted `iterable` property works when built by interpreted code. The AOT-built twin of every
failing case works. Two consumers fail in two different ways from the same cause: `iterator_to_array`
segfaults, `foreach` HANGS forever -- pick one and stay with it, switching between them wastes a
build.

A TRAP THIS UNCOVERED, and it is why the fix must come with a compile check. Once `instanceof` on
an `iterable` operand stopped folding to a constant false (the companion bug_fix packet), the
narrowed branch became live and the backend immediately refused with
`unsupported EIR backend feature: local load from PHP type Iterable as Object("Iterator")`. That
pairing had simply never been reachable before. It is a MOVE, not a conversion -- an `iterable`
slot holds the unboxed payload, so once `instanceof` proves the Traversable half the pointer
already IS the object pointer -- and it is now handled by adding `(Iterable, Object)` and
`(Object, Iterable)` to `local_load_types_share_storage` (src/codegen/lower_inst/local_loads.rs).
Expect more dead branches to wake up the same way as this area is fixed further.

WHERE TO START. Compare what `eval_generator_new` produces against what the AOT Generator method
bodies expect; codegen special-cases each Generator method (see `builtin_iterators.rs`, whose stub
methods exist only so the checker can resolve them). Either those bodies must detect an
eval-owned generator and route through the bridge -- the way `lower_instanceof` already consults
`emit_eval_object_is_a_named_fallback` for eval-owned objects -- or the interpreter must hand back
a generator carrying a real AOT activation.
Evidence: scratchpad/red10: `php -n` 8.5.10 prints `AOT  valid:1 current:1 then:2` and `EVAL valid:1 current:1 then:2`; elephc prints the AOT line identically and then `EVAL` + exit 138. scratchpad/red9 (the same failure one layer up, through a promoted `iterable` property on an eval-built object): elephc gives `read-ok` then exit 139 on `iterator_to_array`, or an infinite hang on `foreach`, against php's `read-ok convert:2 foreach:2`; crash report main-2026-09-12-195752.ips carries KERN_INVALID_ADDRESS 0x0000000100000020, the same address as the Symfony worker's index-2026-09-12-195528.ips whose symbolicated frames are `__rt_mixed_unbox` / `method___Symfony_N_Component_N_Config_N_ResourceCheckerConfigCache___isfresh` / `__elephc_eval_method___...`. Negative controls: scratchpad/red8 (`iterator_to_array` on AOT- and eval-declared generators, driven from interpreted code) prints `aot:2 eval:2` under both; scratchpad/red5 (arrays in the same promoted property, eval-built) matches php. Regression slices after the local_loads change: `generator` 96 passed / 0 failed; `instanceof` 59/2 and `iterable` 120/13, and BOTH of those failure sets reproduce identically on a pristine tree, so they are pre-existing.
Verified by: php -n 8.5.10 as oracle on a 25-line reducer plus three narrowing controls, the macOS crash reports for reducer and Symfony worker sharing one fault address, and the generator/instanceof/iterable codegen slices compared against a pristine tree

## Verification

scratchpad/red10: `php -n` 8.5.10 prints `AOT  valid:1 current:1 then:2` and `EVAL valid:1 current:1 then:2`; elephc prints the AOT line identically and then `EVAL` + exit 138. scratchpad/red9 (the same failure one layer up, through a promoted `iterable` property on an eval-built object): elephc gives `read-ok` then exit 139 on `iterator_to_array`, or an infinite hang on `foreach`, against php's `read-ok convert:2 foreach:2`; crash report main-2026-09-12-195752.ips carries KERN_INVALID_ADDRESS 0x0000000100000020, the same address as the Symfony worker's index-2026-09-12-195528.ips whose symbolicated frames are `__rt_mixed_unbox` / `method___Symfony_N_Component_N_Config_N_ResourceCheckerConfigCache___isfresh` / `__elephc_eval_method___...`. Negative controls: scratchpad/red8 (`iterator_to_array` on AOT- and eval-declared generators, driven from interpreted code) prints `aot:2 eval:2` under both; scratchpad/red5 (arrays in the same promoted property, eval-built) matches php. Regression slices after the local_loads change: `generator` 96 passed / 0 failed; `instanceof` 59/2 and `iterable` 120/13, and BOTH of those failure sets reproduce identically on a pristine tree, so they are pre-existing.

# Citations

[1] explicit_capture (2026-09-12T18:13:31.252Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:gotcha:open-driving-an-interpreter-created-generator-from-aot-code-crashes-25-line-redu","title":"OPEN: driving an INTERPRETER-created Generator from AOT code crashes; 25-line reducer with no object and no property, and it is the Symfony --web SIGSEGV","summary":"OPEN, measured and NOT fixed. This is the root of the Symfony web SIGSEGV, reduced as far as it goes: no object, no property, no web. THE REDUCER, 25 lines. An AOT function drives a generator through the Iterator protoco","body":"OPEN, measured and NOT fixed. This is the root of the Symfony `--web` SIGSEGV, reduced as far as it goes: no object, no property, no --web.\n\nTHE REDUCER, 25 lines. An AOT function drives a generator through the Iterator protocol:\n    function drive(iterable $g): string {\n        if (!($g instanceof Iterator)) { return 'not-iterator'; }\n        $out = 'valid:' . ($g->valid() ? '1' : '0');\n        $out .= ' current:' . $g->current();\n        $g->next();\n        return $out . ' then:' . $g->current();\n    }\nCalled with a generator whose function is declared in AOT code: correct, `valid:1 current:1 then:2`,\nmatching `php -n` 8.5.10. Called with a generator whose function is declared in a runtime-included\n(interpreted) file: CRASH, exit 138, where php prints the same line. So AOT `Generator::valid` /\n`current` / `next` assume an AOT generator activation, and an interpreter-owned generator has none.\n\nTHIS EXPLAINS THE WHOLE CHAIN. Symfony's `ResourceCheckerConfigCache::isFresh()` calls\n`iterator_to_array($this->resourceCheckers)`, and the container feeds that property a\n`RewindableGenerator` built by interpreted code; the compiled `isFresh` body then drives it and\ndies in `__rt_mixed_unbox` at KERN_INVALID_ADDRESS 0x0000000100000020. The earlier packet framing\nthis as \"a promoted `iterable` property on an eval-constructed object\" was one layer too high --\nthe property and the object are incidental. What matters is only WHO CREATED THE GENERATOR.\n\nPROGRESSIVELY NARROWED, so do not redo these: `iterator_to_array()` on an eval-declared generator\ncalled from INTERPRETED code works (prints 2, like php). The same AOT class holding an ARRAY in\nits promoted `iterable` property works when built by interpreted code. The AOT-built twin of every\nfailing case works. Two consumers fail in two different ways from the same cause: `iterator_to_array`\nsegfaults, `foreach` HANGS forever -- pick one and stay with it, switching between them wastes a\nbuild.\n\nA TRAP THIS UNCOVERED, and it is why the fix must come with a compile check. Once `instanceof` on\nan `iterable` operand stopped folding to a constant false (the companion bug_fix packet), the\nnarrowed branch became live and the backend immediately refused with\n`unsupported EIR backend feature: local load from PHP type Iterable as Object(\"Iterator\")`. That\npairing had simply never been reachable before. It is a MOVE, not a conversion -- an `iterable`\nslot holds the unboxed payload, so once `instanceof` proves the Traversable half the pointer\nalready IS the object pointer -- and it is now handled by adding `(Iterable, Object)` and\n`(Object, Iterable)` to `local_load_types_share_storage` (src/codegen/lower_inst/local_loads.rs).\nExpect more dead branches to wake up the same way as this area is fixed further.\n\nWHERE TO START. Compare what `eval_generator_new` produces against what the AOT Generator method\nbodies expect; codegen special-cases each Generator method (see `builtin_iterators.rs`, whose stub\nmethods exist only so the checker can resolve them). Either those bodies must detect an\neval-owned generator and route through the bridge -- the way `lower_instanceof` already consults\n`emit_eval_object_is_a_named_fallback` for eval-owned objects -- or the interpreter must hand back\na generator carrying a real AOT activation.\nEvidence: scratchpad/red10: `php -n` 8.5.10 prints `AOT  valid:1 current:1 then:2` and `EVAL valid:1 current:1 then:2`; elephc prints the AOT line identically and then `EVAL` + exit 138. scratchpad/red9 (the same failure one layer up, through a promoted `iterable` property on an eval-built object): elephc gives `read-ok` then exit 139 on `iterator_to_array`, or an infinite hang on `foreach`, against php's `read-ok convert:2 foreach:2`; crash report main-2026-09-12-195752.ips carries KERN_INVALID_ADDRESS 0x0000000100000020, the same address as the Symfony worker's index-2026-09-12-195528.ips whose symbolicated frames are `__rt_mixed_unbox` / `method___Symfony_N_Component_N_Config_N_ResourceCheckerConfigCache___isfresh` / `__elephc_eval_method___...`. Negative controls: scratchpad/red8 (`iterator_to_array` on AOT- and eval-declared generators, driven from interpreted code) prints `aot:2 eval:2` under both; scratchpad/red5 (arrays in the same promoted property, eval-built) matches php. Regression slices after the local_loads change: `generator` 96 passed / 0 failed; `instanceof` 59/2 and `iterable` 120/13, and BOTH of those failure sets reproduce identically on a pristine tree, so they are pre-existing.\nVerified by: php -n 8.5.10 as oracle on a 25-line reducer plus three narrowing controls, the macOS crash reports for reducer and Symfony worker sharing one fault address, and the generator/instanceof/iterable codegen slices compared against a pristine tree","type":"gotcha","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","symfony-web","generator","eval-bridge","segfault","aot-boundary","open-bug"],"paths":["src/types/checker/builtin_iterators.rs","src/codegen/lower_inst/local_loads.rs","crates/elephc-magician/src/interpreter/generators.rs","examples/symfony-app/vendor/symfony/config/ResourceCheckerConfigCache.php"],"stack":["rust","php","symfony","macos-aarch64"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-12T18:13:31.252Z"}],"context":{"fact":"OPEN, measured and NOT fixed. This is the root of the Symfony `--web` SIGSEGV, reduced as far as it goes: no object, no property, no --web.","verification":"scratchpad/red10: `php -n` 8.5.10 prints `AOT  valid:1 current:1 then:2` and `EVAL valid:1 current:1 then:2`; elephc prints the AOT line identically and then `EVAL` + exit 138. scratchpad/red9 (the same failure one layer up, through a promoted `iterable` property on an eval-built object): elephc gives `read-ok` then exit 139 on `iterator_to_array`, or an infinite hang on `foreach`, against php's `read-ok convert:2 foreach:2`; crash report main-2026-09-12-195752.ips carries KERN_INVALID_ADDRESS 0x0000000100000020, the same address as the Symfony worker's index-2026-09-12-195528.ips whose symbolicated frames are `__rt_mixed_unbox` / `method___Symfony_N_Component_N_Config_N_ResourceCheckerConfigCache___isfresh` / `__elephc_eval_method___...`. Negative controls: scratchpad/red8 (`iterator_to_array` on AOT- and eval-declared generators, driven from interpreted code) prints `aot:2 eval:2` under both; scratchpad/red5 (arrays in the same promoted property, eval-built) matches php. Regression slices after the local_loads change: `generator` 96 passed / 0 failed; `instanceof` 59/2 and `iterable` 120/13, and BOTH of those failure sets reproduce identically on a pristine tree, so they are pre-existing."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-12T18:13:31.252Z","path_fingerprints":[{"path":"src/types/checker/builtin_iterators.rs","sha256":"03c458f5089444c5b4f63f37395e3ad90f7a5f031a3b23cab16a537704b3e4ef","size":8398},{"path":"src/codegen/lower_inst/local_loads.rs","sha256":"d2159140e13fbb18bba4d9352aeaacc614b33e795d2d91b4d7fa2ace57b05bfd","size":11205},{"path":"crates/elephc-magician/src/interpreter/generators.rs","sha256":"856f5f3567ad004147f370c85af2f8d377e568a57f552ac35ec76ecce4f0d8dc","size":37150},{"path":"examples/symfony-app/vendor/symfony/config/ResourceCheckerConfigCache.php","sha256":"3bdcc6c1d622e24a7c799fe4d23bb1a17d8b6e1a3d530c5095f847c239a217f5","size":6426}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":8000,"discovery_tokens_estimated":true,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"stale_reasons":[],"estimated_tokens_saved":1196},"created_at":"2026-09-12T18:13:31.252Z","updated_at":"2026-09-12T18:13:31.252Z","author_branch":"reconcile/dirname-symfony"}
```

