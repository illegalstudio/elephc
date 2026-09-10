---
title: "mb_ereg() - internals"
description: "Compiler internals for mb_ereg(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 810
---

## `mb_ereg()` - internals

## Where it lives

- **Signature**: [`src/builtins/string/mb_ereg.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/mb_ereg.rs)
- **Lowering**: [`src/builtins/semantics.rs`:643](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L643) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.mb_ereg` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `non_heap`
- **Effects**: `static (17 declared effects)`
- **Requirements**: `static (2 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.mb_ereg`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function mb_ereg(string $pattern, string $string, mixed $matches = null): bool
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).
- **By-reference parameters**: `$matches`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/mb_ereg.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_ereg.rs) (`eval_builtin!`)
- **Execution**: shared generated-runtime ABI (`RuntimeBuiltinId(80)`).
- **Dispatch hooks**: _none_ (shared runtime dispatch)
- **By-reference parameters**: `$matches`.

## Cross-references

- [User reference for `mb_ereg()`](../../../php/builtins/string/mb_ereg.md)
