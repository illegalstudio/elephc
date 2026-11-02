---
id: gotcha-open-a-widened-by-ref-array-argument-is-released-9302d4ca
type: gotcha
title: "OPEN: a widened by-ref array argument is released after a static call, freeing the caller's array"
description: "lower_by_ref_array_arg_with_signature stores array_to_mixed back into the local and reloads it; the call then releases that borrowed reload"
created: 2026-09-26
sources:
  - path: src/ir_lower/expr/call_arg_coercion.rs
    blob: 7ac195eb7390817b408fd2a2dec52006259fac05
---

# OPEN: a widened by-ref array argument is released after a static call, freeing the caller's array

## Fact

Reduction (compiled, SIGSEGV in __rt_implode on $seen): scratchpad cowc8.php

    final class CowC { public static function rec(string $id, array &$seen, array $path = [], bool $flag = true): void {
        $path[$id] = $flag; $seen[] = implode(",", array_keys($path));
        if ($id === "a") { self::rec("b", $seen, $path, false); self::rec("c", $seen, $path); } unset($path[$id]); } }
    $seen = []; CowC::rec("a", $seen); echo implode(" | ", $seen), "\n";
    eval('$f = function (array $p) { return count($p); }; echo $f([1]);');

Without the eval line it passes. IR with it: `v45 load_local; v46 array_to_mixed v45;
store_local v46; v47 load_local; static_method_call ... v47 ...; release v47` -- that release
drops $seen's only reference. Without the eval the same position is `nop`: the eval presence
leaves $seen's slot array<never>, so by_ref_array_arg_needs_mixed_storage fires and
lower_by_ref_array_arg_with_signature hands back a borrowed reload.
Next step: find which call-argument release helper classifies that reload as an owning
temporary. Reproduces with the 6e2f1ca811 compiler, so it predates 2026-09-26.

## Why

Pre-existing crash in the caller's next read of the array; it only appears when the slot needs widening
