---
id: decision-compiling-symfony-generated-di-container-aot-is--fa4dfa3d
type: decision
title: "Compiling Symfony generated DI container AOT is the 36ms lever, and the blocker is backend spread lowering"
description: "the frontend half type-checks clean; what remains is spread lowering over a callable-typed parameter"
tags: [symfony, performance, lowering, spread]
created: 2026-09-15
verified_by: "measured 2026-09-15 with examples/symfony-app/public/index_preload.php"
stale_when: "lower_positional_spread_args_with_signature learns to route callable spreads through the interpreter bridge, or preload.rs starts treating the preload list as a compile input"
sources:
  - path: src/ir_lower/expr/positional_spreads.rs
    blob: bf4fad58fbaa95eb3545093d498fa05fcdd0ac86
    lines: 13-88
    snip: 3f93428de048
    anchor: "pub(super) fn lower_positional_spread_args_with_signature("
  - path: src/codegen/lower_inst/builtins/pointers.rs
    blob: 29374550ff36424eaad358646c4f0b5062cd28e8
    lines: 171-225
    snip: c3b58b18b0ea
    anchor: "pub(crate) fn lower_elephc_normalize_callable("
  - path: src/ir_lower/stmt/conditionals.rs
    blob: 5d239a0ea6401fc83a0a5b6640853b60b7e7daaa
    lines: 150-186
    snip: 0a668cfe2f5c
    anchor: "fn finish_if_type_join("
  - path: src/opcache_prelude/preload.rs
    blob: 6de65bb1e20bd6d63d0229cb51dfa00c3496b0ef
---

# Compiling Symfony generated DI container AOT is the 36ms lever, and the blocker is backend spread lowering

## Fact

Symfony's `--web` request spends ~36 ms almost entirely in the interpreter because the
generated DI container (`var/cache/<env>/ContainerXXXX/*.php`) is loaded by a runtime-computed
`include`, so the compiler never reads it and every service class it names as a string literal
stays interpreted.

Compiling it is feasible and INCREMENTAL: the container's own `load()` starts with
`class_exists($class, false)`, so any fragment elephc compiles AOT short-circuits there and is
never included at run time. Verified 2026-09-15 that a dual-included file (compiled AOT *and*
`require_once`d at run time) does not double-declare, and that `class_exists($name, false)`
plus a dynamic `$class::do(...)` both reach the compiled class.

Measured 2026-09-15 by statically `require_once`-ing the container fragments from a copy of
`public/index.php`: **22 compile errors** for the main container class alone, **145** across all
105 fragments — type-checker gaps, not parse gaps. The main container class now type-checks
with **zero** errors (22 -> 0).

## Where the blocker actually is

It is in the BACKEND, not the checker. EIR lowering fails on `method_exists(...$controller)` in
`Symfony\Component\HttpKernel\Event\ControllerEvent`, because the spread source is a
`callable`-typed parameter. The checker narrows it to an array through
`\is_array($controller) && ...`, but lowering reads `ctx.local_type()`, still sees `Callable`,
so `lower_positional_spread_args_with_signature` bails, the spread is passed as ONE operand,
and EIR validation reports `OperandCountMismatch` with no source line.
Minimal repro: `scratchpad/spreadcallable2.php`.

## Why carrying the narrowing into lowering would be wrong

The slot physically holds a callable DESCRIPTOR, built by the normalize-callable path at the
`mixed -> callable` parameter boundary, not the `[$obj, 'method']` array: every non-`Callable`
source is converted to a descriptor. Re-typing it would reinterpret a descriptor as an array.

Rebuilding the array FROM the descriptor is not generally possible either — the descriptor
resolves the method at COMPILE time into an invoker pointer plus a capture environment, so the
method-name string need not survive.

The plausible answer is the one elephc already uses for `is_array()` on a `callable` (which is
why that probe drags in the eval bridge): route the spread through the interpreter bridge
rather than lowering it natively.

## Unrelated observation from the same probe

A tiny program that calls `is_array()` on a `callable` parameter drags in the PHAR/zlib/bzip2
bridge and fails to link (`_inflate`, `_BZ2_bzBuffToBuffDecompress`, `_elephc_phar_*`
undefined): the code emits the references but nothing records the link requirement.
Repro `scratchpad/callshape2.php`. Unrelated to the spread.

## Correction to an earlier note

An earlier version of this memory named the branch-narrowing precedent
`apply_instanceof_branch_narrowing`. **No such symbol exists.** Line 166 of
`src/ir_lower/stmt/conditionals.rs` falls inside `finish_if_type_join` (L150-L186), which is
the real if-arm type-join code, alongside `join_arm_types` (L189-L227).

## Trigger

working on Symfony web request performance, opcache preload, or OperandCountMismatch on a spread

## Apply

Rebuild the experiment with examples/symfony-app/public/index_preload.php (untracked): index.php plus a require_once of the container class. The standardized production mechanism would be to honor opcache.preload as a compile input rather than only validating it.
