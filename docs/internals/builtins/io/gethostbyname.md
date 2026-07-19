---
title: "gethostbyname() — internals"
description: "Compiler internals for gethostbyname(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 182
---

## `gethostbyname()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/gethostbyname.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/gethostbyname.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:3416](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L3416) (`lower_gethostbyname`)
- **Function symbol**: `lower_gethostbyname()`


### Lowering notes

- Lowers `gethostbyname(hostname)` through the shared runtime resolver.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_gethostbyaddr`
- `__rt_gethostbyname`

## Signature summary

```php
function gethostbyname(string $hostname): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/network_env/gethostbyname.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/gethostbyname.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `gethostbyname()`](../../../php/builtins/io/gethostbyname.md)
