---
title: "preg_match_all() — internals"
description: "Compiler internals for preg_match_all(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 341
---

## `preg_match_all()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/preg_match_all.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/preg_match_all.rs)
- **Lowering**: [`src/builtins/semantics.rs`:425](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L425) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.preg_match_all` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `may_alias_arguments`
- **Effects**: `static (16 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.preg_match_all`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function preg_match_all(string $pattern, string $subject): int
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/regex/preg_match_all.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/regex/preg_match_all.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`
- **By-reference parameters**: `$matches`.

## Cross-references

- [User reference for `preg_match_all()`](../../../php/builtins/regex/preg_match_all.md)
