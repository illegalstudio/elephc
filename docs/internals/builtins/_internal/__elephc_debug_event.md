---
title: "__elephc_debug_event() - internals"
description: "Compiler internals for __elephc_debug_event(): lowering path, type checks, and runtime helpers."
---

## `__elephc_debug_event()` - internals

## Where it lives

- **Signature**: [`src/builtins/debug/event.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/debug/event.rs)
- **Lowering**: [`src/builtins/semantics.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.__elephc_debug_event` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `non_heap`
- **Effects**: `static (5 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.__elephc_debug_event`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function __elephc_debug_event(int $kind, mixed $value, ?string $label = null, string $to = 'auto'): void
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- _No user-facing reference - this is a compiler internal helper._
