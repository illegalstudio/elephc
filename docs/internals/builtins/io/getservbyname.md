---
title: "getservbyname() — internals"
description: "Compiler internals for getservbyname(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 186
---

## `getservbyname()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/getservbyname.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/getservbyname.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:3482](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L3482) (`lower_getservbyname`)
- **Function symbol**: `lower_getservbyname()`


### Lowering notes

- Lowers `getservbyname(service, protocol)` and boxes a missing entry as PHP `false`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_getservbyname`

## Signature summary

```php
function getservbyname(string $service, string $protocol): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/network_env/getservbyname.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/getservbyname.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `getservbyname()`](../../../php/builtins/io/getservbyname.md)
