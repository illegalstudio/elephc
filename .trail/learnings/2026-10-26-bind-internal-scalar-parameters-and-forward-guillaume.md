---
title: Bind internal scalar parameters and forward builtins through preludes
author: Guillaume
date: 2026-10-26
branch: fix/random-followups-2082-2083-2084
touches:
  - docs/internals/the-runtime.md
---

## Decided
- `Mt19937`'s seed follows php-src's `Z_PARAM_LONG_OR_NULL` in the constructor body, not through a typed parameter: it binds `mixed $seed` with `__elephc_random_mt19937_seed($seed, __elephc_call_strict_types())`. The constructing file's strictness reaches that internal builtin through a runtime slot: every construction site emits `CoreBuiltinOp::PublishCallStrictness` (after argument evaluation, so a nested construction cannot overwrite it), reflection publishes `false` because php-src treats it as an internal caller, and the slot zero-initializes to weak. A name-resolver rewrite cannot see dynamic class names or spread sites; the slot can.
- A prelude constructor that needs the caller's file profile has no PHP-visible way to read it: the value must be published by lowering at each call site. Publication after argument evaluation is required, since argument lowering can itself construct objects from another file.
- Integer-string seed parsing must follow `zend_strtol`, not a double comparison: `"9223372036854775808"` is a `TypeError`, while `"-9223372036854775809"` saturates to `PHP_INT_MIN` because the overflow falls back to the (in-range) double value.
- A registry builtin lowered onto a prelude helper is only one implementation if every call form reaches it. `BuiltinLowering::Eir` forwards string callables to `__elephc_array_rand`, but such a builtin must stay `BuiltinCallablePolicy::StaticOnly`: the runtime callable wrapper cannot compose a user call (its `emit_user_call` returns a null sentinel, which crashes).
- A helper referenced only from EIR has no AST root, so reachability prunes it. `PreludeInventory::record_required_function` roots it, recorded by the prelude when the program names `array_rand`.
- php-src's bad-scaling formula casts the fractional offset to `zend_ulong` before adding `min` with wrapping arithmetic; do it in Rust (`Mt19937::legacy_scale`) and share it through the bridge with the Randomizer prelude instead of re-deriving it in PHP ints (which overflow to float).

## Tried and ruled out
- Declaring the seed `?int`: the checker rejects weak scalar literals for declared user/prelude params and the EIR cannot convert Float/Str to a `TaggedScalar` operand, so spread and dynamic constructions stopped compiling.
- Comparing `(int)$seed != $seed` for integer-string range checks: numeric int/string comparison goes through a double above 2^53 and accepts out-of-range values.
- Leaving `BuiltinCallablePolicy::Dynamic` on the forwarded builtin: `call_user_func("array_rand", array: ...)` segfaults through the wrapper stub.
- Implementing the offset addition in PHP prelude ints: `int + int` overflows to float, unlike `zend_ulong`.

## Watch out
- `current_strict_types()` is only trustworthy while a statement's profile is installed (name resolver, EIR statement lowering, optimizer passes do this), so publish its value during lowering instead of reading it at run time.
- A `WRITES_GLOBAL` publish instruction is never DCE'd or hoisted (only pure ops are), and its reader must be `READS_GLOBAL` for the same reason.
- `docs/internals/the-runtime.md` documents runtime helpers by name; deleting a helper needs that table updated too.
- The reachability root must be recorded when the prelude is injected, before pruning.
