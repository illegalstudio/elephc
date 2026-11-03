---
title: "exec() - internals"
description: "Compiler internals for exec(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 863
---

## `exec()` - internals

## Where it lives

- **Signature**: [`src/builtins/system/exec.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/exec.rs)
- **Lowering**: [`src/builtins/semantics.rs`:752](https://github.com/illegalstudio/elephc/blob/main/src/builtins/semantics.rs#L752) (`lower_registry_call`)
- **Function symbol**: `lower_registry_call()`


### Lowering notes

- Uses the `runtime_call` strategy from the single-source builtin descriptor.
- Emits the typed EIR target `runtime.exec` through `BuiltinLoweringContext`.
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

- **Typed EIR target**: `runtime.exec`
- **Backend boundary**: `src/codegen/lower_inst/runtime_calls.rs` resolves the typed target without PHP-name dispatch.

## Signature summary

```php
function exec(string $command, array $output = null, int $result_code = null): string
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).
- **By-reference parameters**: `$output`, `$result_code`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/network_env/exec.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/exec.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `by-reference-or-lvalue`.
- **Dispatch hooks**: `direct`, `values`
- **By-reference parameters**: `$output`, `$result_code`.

## Cross-references

- [User reference for `exec()`](../../../php/builtins/process/exec.md)
