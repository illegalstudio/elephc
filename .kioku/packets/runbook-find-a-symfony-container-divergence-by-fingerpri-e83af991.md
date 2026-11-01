---
id: runbook-find-a-symfony-container-divergence-by-fingerpri-e83af991
type: runbook
title: "Find a Symfony container divergence by fingerprinting every compiler pass, php vs compiled"
description: "Replay the kernel's compiler passes one by one in a probe, dump a sorted definition+alias fingerprint per pass, and diff the first pass whose md5 differs"
created: 2026-09-26
sources:
  - path: examples/symfony-app/elephc-probe-passes.php
    blob: 22582dfd3bb5d7afd58a06b316fbbb8f85314a01
  - path: src/codegen/lower_inst/iterators.rs
    blob: 9277586daaf5eeba9dd70b12f13984689235b542
    lines: 1768-1790
    snip: 54c32c178c64
    anchor: "fn emit_detach_hash_value_box(ctx: &mut FunctionContext<'_>) {"
  - path: src/ir_lower/expr/assoc_array_literals.rs
    blob: 8d84d0e93e1278917c62d8903a4381f961d68b34
    lines: 250-260
    snip: 3fede6929215
    anchor: "ExprKind::ArrayLiteralAssoc(pairs) => Some(assoc_array_literal_type_for_ir(ctx, pairs, array)),"
  - path: src/ir_lower/function.rs
    blob: eb23061fd6d855bcf9281171496d64b8e8fbae2f
    lines: 1390-1400
    snip: 6a9076cb000d
    anchor: "if name == \"this\" {"
  - path: src/ir_lower/context.rs
    blob: ed0bb5107d520b04a073b4254e76d2a50e60eb00
    lines: 1071-1095
    snip: b63e96e75a4b
    anchor: "pub(crate) fn privatize_mixed_param("
---

# Find a Symfony container divergence by fingerprinting every compiler pass, php vs compiled

## Fact

When a Symfony container diverges (cache:clear dumps, missing services, inlined ids), do not read
the dumped PHP diff first: fingerprint the ContainerBuilder after EVERY compiler pass on both
sides and take the first pass whose fingerprint differs.

Probe (kept in the app dir, never in vendor): examples/symfony-app/elephc-probe-passes.php.
It does `(new ReflectionMethod($kernel, 'initializeBundles'))->invoke($kernel)` then
`buildContainer()`, then runs `$container->getCompiler()->getPassConfig()->getPasses()` one by
one, writing one sorted text file per pass: every definition (class, public, shared, factory,
arguments, method calls, properties, TAGS WITH ATTRIBUTES) and every alias. The summary line per
pass is its md5. Run it under `php` and compiled (APP_ENV=prod, restore var/ afterwards), diff
the two summaries, then diff the two files of the first differing pass.

Include tag ATTRIBUTES, not just tag names: the first run only listed names and pointed at pass
33 (ResettableServicePass) when the real divergence was pass 10 (CachePoolPass) writing tag
attributes.

On 2026-09-26 this found, in order, each a runtime defect with a small reduction:
- pass 10 CachePoolPass: a by-value foreach over a hash handed the loop variable the array's own
  Mixed cell (iterators.rs reuse_box), and `['method' => $tags[0][$attr]]` was stamped
  array<string,int> (assoc literal typing did not recurse into nested array access);
- pass 43 ResolveParameterPlaceHoldersPass: a by-value `mixed` parameter shared the caller's
  cell, so `processValue()` recursion wrote into the caller's `$v` (function.rs now privatizes
  written Mixed params with MixedClone);
- after all 90 passes matched, the dump itself: `new \ReflectionClass($class)` folded a declared
  `?\ReflectionClass &$class` holding a string into a constant `ReflectionClass`.
Then cache:clear itself needed: closures callable across eval contexts, stdClass dynamic property
writes from eval retaining their value, str_replace's by-ref $count in eval, and an interpreter
foreach snapshot.

## Why

Reading the dumped container diff shows symptoms several passes downstream; the per-pass fingerprint names the defect's pass directly, and every divergence found so far was a generic runtime defect
