---
title: "getservbyport() — internals"
description: "Compiler internals for getservbyport(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 187
---

## `getservbyport()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/getservbyport.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/getservbyport.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:3514](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L3514) (`lower_getservbyport`)
- **Function symbol**: `lower_getservbyport()`


### Lowering notes

- Lowers `getservbyport(port, protocol)` and boxes a missing entry as PHP `false`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_getservbyport`

## Signature summary

```php
function getservbyport(int $port, string $protocol): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/network_env/getservbyport.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/getservbyport.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `getservbyport()`](../../../php/builtins/io/getservbyport.md)
