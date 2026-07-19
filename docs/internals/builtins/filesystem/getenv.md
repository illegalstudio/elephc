---
title: "getenv() — internals"
description: "Compiler internals for getenv(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 124
---

## `getenv()` — internals

## Where it lives

- **Signature**: [`src/builtins/system/getenv.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/system/getenv.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/system.rs`:699](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/system.rs#L699) (`lower_getenv`)
- **Function symbol**: `lower_getenv()`


### Lowering notes

- Lowers `getenv(name)` through the target-aware environment lookup helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_getenv`

## Signature summary

```php
function getenv(string $name): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/network_env/getenv.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/getenv.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `getenv()`](../../../php/builtins/filesystem/getenv.md)
