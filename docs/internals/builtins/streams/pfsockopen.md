---
title: "pfsockopen() — internals"
description: "Compiler internals for pfsockopen(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 358
---

## `pfsockopen()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/pfsockopen.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/pfsockopen.rs)
- **Lowering**: [`src/builtins/semantics.rs`:425](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L425) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.pfsockopen` through `BuiltinLoweringContext`.
- The backend resolves that typed target through `src/codegen/lower_inst/runtime_calls.rs`; PHP builtin names do not participate in dispatch.

## Semantic descriptor

- **Target strategy**: `runtime_call`
- **Validation**: `checker_hook`
- **Result type source**: `checked`
- **Result ownership**: `may_alias_arguments`
- **Effects**: `static (16 declared effects)`
- **Requirements**: `static (0 requirements)`
- **Callable policy**: `static_only`
- **Target support**: `macos-aarch64`, `linux-aarch64`, `linux-x86_64`

## EIR and runtime boundary

- **Typed EIR target**: `runtime.pfsockopen`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function pfsockopen(string $hostname, int $port, int &$error_code = null, string &$error_message = null, float $timeout = null): mixed
```

## What the type checker enforces

- **Arity**: takes 2–5 arguments (3 optional).
- **By-reference parameters**: `$error_code`, `$error_message`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/pfsockopen.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/pfsockopen.rs) (`eval_builtin!`)
- **Dispatch hooks**: `values`
- **By-reference parameters**: `$error_code`, `$error_message`.

## Cross-references

- [User reference for `pfsockopen()`](../../../php/builtins/streams/pfsockopen.md)
