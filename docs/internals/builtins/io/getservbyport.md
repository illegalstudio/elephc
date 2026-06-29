---
title: "getservbyport() — internals"
description: "Compiler internals for getservbyport(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 166
---

## `getservbyport()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3515](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3515) (`lower_getservbyport`)
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

## Cross-references

- [User reference for `getservbyport()`](../../../php/builtins/io/getservbyport.md)

