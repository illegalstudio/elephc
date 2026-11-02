---
type: "Gotcha"
title: "OPEN: a promoted `iterable` property on an EVAL-constructed object is unusable from AOT code -- iterator_to_array segfaults, foreach hangs -- and this is the Symfony --web SIGSEGV"
description: "OPEN, measured and NOT fixed, with a ~40 line reducer. This IS the crash that blocks the Symfony web request; the reducer reproduces its exact fault address. THE REDUCER, and every step is separated so the failing one na"
resource: "src/ir_lower/gradual_coercions.rs"
tags: ["session-learning", "symfony-web", "segfault", "iterable", "promoted-property", "eval-bridge", "representation", "open-bug"]
timestamp: "2026-09-12T18:03:55.500Z"
x-kage-id: "repo:lazy-petting-popcorn:gotcha:open-a-promoted-iterable-property-on-an-eval-constructed-object-is-unusable-from"
x-kage-type: "gotcha"
x-kage-status: "approved"
x-kage-scope: "repo"
x-kage-visibility: "team"
x-kage-confidence: 0.7
x-kage-verified: "verified"
x-kage-paths: ["src/ir_lower/gradual_coercions.rs", "src/types/model.rs", "crates/elephc-magician/src/interpreter/statements/native_constructor_defaults.rs", "examples/symfony-app/vendor/symfony/config/ResourceCheckerConfigCache.php"]
x-kage-stack: ["rust", "php", "symfony", "macos-aarch64"]
---

# OPEN: a promoted `iterable` property on an EVAL-constructed object is unusable from AOT code -- iterator_to_array segfaults, foreach hangs -- and this is the Symfony --web SIGSEGV

> OPEN, measured and NOT fixed, with a ~40 line reducer. This IS the crash that blocks the Symfony web request; the red…

OPEN, measured and NOT fixed, with a ~40-line reducer. This IS the crash that blocks the Symfony `--web` request; the reducer reproduces its exact fault address.

THE REDUCER, and every step is separated so the failing one names itself. An AOT class
    class Holder { public function __construct(private iterable $items = []) {}
      step1_read()    { $v = $this->items; return 'read-ok'; }
      step2_convert() { return count(iterator_to_array($this->items)); }
      step3_foreach() { $n = 0; foreach ($this->items as $_) { $n++; } return $n; } }
is built TWICE with a two-element generator: once in AOT code, once by a runtime-included
(interpreted) file. php -n 8.5.10 gives `read-ok convert:2 foreach:2` for both.
elephc:
  AOT-built  : read-ok convert:2 foreach:2      <- correct
  EVAL-built : read-ok                          <- the property READ is fine
               convert -> SIGSEGV, exit 139, KERN_INVALID_ADDRESS at 0x0000000100000020
               foreach -> HANGS (killed at 300 s), it does not crash
So the value in the slot is not merely wrong, it is inconsistent: reading it back is fine, but
every consumer misbehaves, and in two different ways.

IT IS THE SAME BUG AS SYMFONY'S. Identical fault address 0x0000000100000020, and the symbolicated
Symfony crash report names the same shape: `__rt_mixed_unbox` <- AOT
`method___...ResourceCheckerConfigCache___isfresh` <- `__elephc_eval_method___...` <- the
interpreter's `native_method_execution`. Symfony's `ResourceCheckerConfigCache` is constructed by
the generated container (interpreted) and declares exactly this member:
`private iterable $resourceCheckers = []`, then `isFresh()` calls `iterator_to_array()` on it.

WHAT IS ALREADY RULED OUT, do not redo. `iterator_to_array()` on a generator is fine on its own,
whether the generator function is declared in AOT code or in the included file (both print 2) --
the object/property path is required. The AOT-constructed twin of the very same class works for
all three steps, so the class shape, the promotion and the `iterable` declaration are not by
themselves the problem; the CONSTRUCTOR being the interpreter is. An array in the property is
also fine: the same `Holder` built by interpreted code with `[]` or `[1,2]` works (scratchpad
red5), so it is specifically the Traversable half.

WHERE THIS POINTS. `iterable` slots hold the UNBOXED payload in AOT (coercing to `iterable`
emits `Op::MixedUnbox`, src/ir_lower/gradual_coercions.rs:104; `src/types/model.rs:120` calls it a
type-erased pointer). The interpreter writes that promoted property when it constructs the
object, and the question to answer first is whether it writes the same representation AOT readers
assume -- a boxed cell where a raw payload is expected would explain a clean read (the pointer is
readable) followed by a garbage dereference at +0x20 in `__rt_mixed_unbox` and an
never-terminating iteration in foreach. Compare what `eval_native_constructor_*` /
the promoted-property write path stores against what `PhpType::Iterable` consumers load.

A NOTE ON THE ORDER OF SYMPTOMS: with `foreach` placed BEFORE the conversion in the interpreted
file, the run hangs instead of crashing, so a session chasing this should pick one consumer and
stick to it -- the two failure modes are the same defect and switching between them wastes a
build.
Evidence: scratchpad/red9 (main.php AOT `Holder` + gen.php included through `getenv('PROBE_INC')`): php -n 8.5.10 prints `AOT read-ok convert:2 foreach:2 / EVAL read:read-ok / EVAL foreach:2 / EVAL convert:2 / after`; elephc prints the AOT line identically, then `EVAL read:read-ok` and dies -- `EVAL convert:` then exit 139 when convert runs second, or hangs at `EVAL foreach:` (killed at 300 s) when foreach runs second. Crash report ~/Library/Logs/DiagnosticReports/main-2026-09-12-195752.ips: KERN_INVALID_ADDRESS at 0x0000000100000020, the same address as the Symfony worker's index-2026-09-12-195528.ips, whose symbolicated frames are `__rt_mixed_unbox` / `method___Symfony_N_Component_N_Config_N_ResourceCheckerConfigCache___isfresh` / `__elephc_eval_method___...` / `native_method_execution::eval_native_method_with_evaluated_args_*`. Negative controls: scratchpad/red8 (`iterator_to_array` on an AOT-declared and on an eval-declared generator) prints `aot:2 eval:2` under both php and elephc; scratchpad/red5 (same Holder shape, arrays instead of a generator, built by interpreted code) matches php exactly.
Verified by: php -n 8.5.10 as oracle on a step-separated reducer plus two negative controls, and the macOS crash reports for both the reducer and the Symfony worker showing one fault address

## Verification

scratchpad/red9 (main.php AOT `Holder` + gen.php included through `getenv('PROBE_INC')`): php -n 8.5.10 prints `AOT read-ok convert:2 foreach:2 / EVAL read:read-ok / EVAL foreach:2 / EVAL convert:2 / after`; elephc prints the AOT line identically, then `EVAL read:read-ok` and dies -- `EVAL convert:` then exit 139 when convert runs second, or hangs at `EVAL foreach:` (killed at 300 s) when foreach runs second. Crash report ~/Library/Logs/DiagnosticReports/main-2026-09-12-195752.ips: KERN_INVALID_ADDRESS at 0x0000000100000020, the same address as the Symfony worker's index-2026-09-12-195528.ips, whose symbolicated frames are `__rt_mixed_unbox` / `method___Symfony_N_Component_N_Config_N_ResourceCheckerConfigCache___isfresh` / `__elephc_eval_method___...` / `native_method_execution::eval_native_method_with_evaluated_args_*`. Negative controls: scratchpad/red8 (`iterator_to_array` on an AOT-declared and on an eval-declared generator) prints `aot:2 eval:2` under both php and elephc; scratchpad/red5 (same Holder shape, arrays instead of a generator, built by interpreted code) matches php exactly.

# Citations

[1] explicit_capture (2026-09-12T18:03:55.500Z)

## Kage state

Machine state for lossless round-trip; OKF consumers can ignore it.

```json kage-state
{"schema_version":2,"id":"repo:lazy-petting-popcorn:gotcha:open-a-promoted-iterable-property-on-an-eval-constructed-object-is-unusable-from","title":"OPEN: a promoted `iterable` property on an EVAL-constructed object is unusable from AOT code -- iterator_to_array segfaults, foreach hangs -- and this is the Symfony --web SIGSEGV","summary":"OPEN, measured and NOT fixed, with a ~40 line reducer. This IS the crash that blocks the Symfony web request; the reducer reproduces its exact fault address. THE REDUCER, and every step is separated so the failing one na","body":"OPEN, measured and NOT fixed, with a ~40-line reducer. This IS the crash that blocks the Symfony `--web` request; the reducer reproduces its exact fault address.\n\nTHE REDUCER, and every step is separated so the failing one names itself. An AOT class\n    class Holder { public function __construct(private iterable $items = []) {}\n      step1_read()    { $v = $this->items; return 'read-ok'; }\n      step2_convert() { return count(iterator_to_array($this->items)); }\n      step3_foreach() { $n = 0; foreach ($this->items as $_) { $n++; } return $n; } }\nis built TWICE with a two-element generator: once in AOT code, once by a runtime-included\n(interpreted) file. php -n 8.5.10 gives `read-ok convert:2 foreach:2` for both.\nelephc:\n  AOT-built  : read-ok convert:2 foreach:2      <- correct\n  EVAL-built : read-ok                          <- the property READ is fine\n               convert -> SIGSEGV, exit 139, KERN_INVALID_ADDRESS at 0x0000000100000020\n               foreach -> HANGS (killed at 300 s), it does not crash\nSo the value in the slot is not merely wrong, it is inconsistent: reading it back is fine, but\nevery consumer misbehaves, and in two different ways.\n\nIT IS THE SAME BUG AS SYMFONY'S. Identical fault address 0x0000000100000020, and the symbolicated\nSymfony crash report names the same shape: `__rt_mixed_unbox` <- AOT\n`method___...ResourceCheckerConfigCache___isfresh` <- `__elephc_eval_method___...` <- the\ninterpreter's `native_method_execution`. Symfony's `ResourceCheckerConfigCache` is constructed by\nthe generated container (interpreted) and declares exactly this member:\n`private iterable $resourceCheckers = []`, then `isFresh()` calls `iterator_to_array()` on it.\n\nWHAT IS ALREADY RULED OUT, do not redo. `iterator_to_array()` on a generator is fine on its own,\nwhether the generator function is declared in AOT code or in the included file (both print 2) --\nthe object/property path is required. The AOT-constructed twin of the very same class works for\nall three steps, so the class shape, the promotion and the `iterable` declaration are not by\nthemselves the problem; the CONSTRUCTOR being the interpreter is. An array in the property is\nalso fine: the same `Holder` built by interpreted code with `[]` or `[1,2]` works (scratchpad\nred5), so it is specifically the Traversable half.\n\nWHERE THIS POINTS. `iterable` slots hold the UNBOXED payload in AOT (coercing to `iterable`\nemits `Op::MixedUnbox`, src/ir_lower/gradual_coercions.rs:104; `src/types/model.rs:120` calls it a\ntype-erased pointer). The interpreter writes that promoted property when it constructs the\nobject, and the question to answer first is whether it writes the same representation AOT readers\nassume -- a boxed cell where a raw payload is expected would explain a clean read (the pointer is\nreadable) followed by a garbage dereference at +0x20 in `__rt_mixed_unbox` and an\nnever-terminating iteration in foreach. Compare what `eval_native_constructor_*` /\nthe promoted-property write path stores against what `PhpType::Iterable` consumers load.\n\nA NOTE ON THE ORDER OF SYMPTOMS: with `foreach` placed BEFORE the conversion in the interpreted\nfile, the run hangs instead of crashing, so a session chasing this should pick one consumer and\nstick to it -- the two failure modes are the same defect and switching between them wastes a\nbuild.\nEvidence: scratchpad/red9 (main.php AOT `Holder` + gen.php included through `getenv('PROBE_INC')`): php -n 8.5.10 prints `AOT read-ok convert:2 foreach:2 / EVAL read:read-ok / EVAL foreach:2 / EVAL convert:2 / after`; elephc prints the AOT line identically, then `EVAL read:read-ok` and dies -- `EVAL convert:` then exit 139 when convert runs second, or hangs at `EVAL foreach:` (killed at 300 s) when foreach runs second. Crash report ~/Library/Logs/DiagnosticReports/main-2026-09-12-195752.ips: KERN_INVALID_ADDRESS at 0x0000000100000020, the same address as the Symfony worker's index-2026-09-12-195528.ips, whose symbolicated frames are `__rt_mixed_unbox` / `method___Symfony_N_Component_N_Config_N_ResourceCheckerConfigCache___isfresh` / `__elephc_eval_method___...` / `native_method_execution::eval_native_method_with_evaluated_args_*`. Negative controls: scratchpad/red8 (`iterator_to_array` on an AOT-declared and on an eval-declared generator) prints `aot:2 eval:2` under both php and elephc; scratchpad/red5 (same Holder shape, arrays instead of a generator, built by interpreted code) matches php exactly.\nVerified by: php -n 8.5.10 as oracle on a step-separated reducer plus two negative controls, and the macOS crash reports for both the reducer and the Symfony worker showing one fault address","type":"gotcha","scope":"repo","visibility":"team","sensitivity":"internal","status":"approved","confidence":0.7,"tags":["session-learning","symfony-web","segfault","iterable","promoted-property","eval-bridge","representation","open-bug"],"paths":["src/ir_lower/gradual_coercions.rs","src/types/model.rs","crates/elephc-magician/src/interpreter/statements/native_constructor_defaults.rs","examples/symfony-app/vendor/symfony/config/ResourceCheckerConfigCache.php"],"stack":["rust","php","symfony","macos-aarch64"],"source_refs":[{"kind":"explicit_capture","captured_at":"2026-09-12T18:03:55.500Z"}],"context":{"fact":"OPEN, measured and NOT fixed, with a ~40-line reducer. This IS the crash that blocks the Symfony `--web` request; the reducer reproduces its exact fault address.","verification":"scratchpad/red9 (main.php AOT `Holder` + gen.php included through `getenv('PROBE_INC')`): php -n 8.5.10 prints `AOT read-ok convert:2 foreach:2 / EVAL read:read-ok / EVAL foreach:2 / EVAL convert:2 / after`; elephc prints the AOT line identically, then `EVAL read:read-ok` and dies -- `EVAL convert:` then exit 139 when convert runs second, or hangs at `EVAL foreach:` (killed at 300 s) when foreach runs second. Crash report ~/Library/Logs/DiagnosticReports/main-2026-09-12-195752.ips: KERN_INVALID_ADDRESS at 0x0000000100000020, the same address as the Symfony worker's index-2026-09-12-195528.ips, whose symbolicated frames are `__rt_mixed_unbox` / `method___Symfony_N_Component_N_Config_N_ResourceCheckerConfigCache___isfresh` / `__elephc_eval_method___...` / `native_method_execution::eval_native_method_with_evaluated_args_*`. Negative controls: scratchpad/red8 (`iterator_to_array` on an AOT-declared and on an eval-declared generator) prints `aot:2 eval:2` under both php and elephc; scratchpad/red5 (same Holder shape, arrays instead of a generator, built by interpreted code) matches php exactly."},"freshness":{"ttl_days":365,"last_verified_at":"2026-09-12T18:03:55.500Z","path_fingerprints":[{"path":"src/ir_lower/gradual_coercions.rs","sha256":"c6f9d05fb9c27cac452f3f3fcf6a4515665f3acbaa9bbc241d11cbf751753b7f","size":5765},{"path":"src/types/model.rs","sha256":"3b375d1544a4838d7ecfc6dfb8f6e7d927d5c1a4daa827ddbac8519149c836a1","size":13695},{"path":"crates/elephc-magician/src/interpreter/statements/native_constructor_defaults.rs","sha256":"c89abf043ec5c1b76c303a63b3010c3422cbcd78b4fff3933318afdc1181a406","size":12938},{"path":"examples/symfony-app/vendor/symfony/config/ResourceCheckerConfigCache.php","sha256":"3bdcc6c1d622e24a7c799fe4d23bb1a17d8b6e1a3d530c5095f847c239a217f5","size":6426}],"path_fingerprint_policy":"source_hash_staleness","verification":"repo_local_agent_capture"},"edges":[],"quality":{"reviewer":"repo-local-agent","votes_up":0,"votes_down":0,"uses_30d":0,"reports_stale":0,"review_boundary":"git_or_pr","promotion_requires_review":true,"discovery_tokens":8000,"discovery_tokens_estimated":true,"score":94,"reasons":["high-value memory type","has source evidence","grounded to repo paths","tagged","actionable rationale or verification"],"risks":[],"duplicate_candidates":[],"stale_reasons":[],"estimated_tokens_saved":1163},"created_at":"2026-09-12T18:03:55.500Z","updated_at":"2026-09-12T18:03:55.500Z","author_branch":"reconcile/dirname-symfony"}
```

