---
title: "strncasecmp() — internals"
description: "Compiler internals for strncasecmp(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 474
---

## `strncasecmp()` — internals

## Where it lives

- **Signature**: [`src/builtins/string/strncasecmp.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/string/strncasecmp.rs)
- **Lowering**: [`src/builtins/semantics.rs`:576](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L576) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.strncasecmp` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `signature`
- **Result type source**: `declared`
- **Result ownership**: `may_alias_arguments`
- **Effects**: `static (1 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `ios-arm64`, `ios-sim-arm64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.strncasecmp`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function strncasecmp(string $string1, string $string2, int $length): int
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `strncasecmp()`](../../../php/builtins/string/strncasecmp.md)
