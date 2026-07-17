---
title: "gethostname() — internals"
description: "Compiler internals for gethostname(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 183
---

## `gethostname()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/gethostname.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/gethostname.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:3409](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L3409) (`lower_gethostname`)
- **Function symbol**: `lower_gethostname()`


### Lowering notes

- Lowers `gethostname()` through the shared runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_gethostbyaddr`
- `__rt_gethostbyname`
- `__rt_gethostname`

## Signature summary

```php
function gethostname(): string
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/network_env/gethostname.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/gethostname.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `gethostname()`](../../../php/builtins/io/gethostname.md)
