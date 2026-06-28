---
title: "gethostbyaddr() — internals"
description: "Compiler internals for gethostbyaddr(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 160
---

## `gethostbyaddr()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3210](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3210) (`lower_gethostbyaddr`)
- **Function symbol**: `lower_gethostbyaddr()`


### Lowering notes

- Lowers `gethostbyaddr(address)` and boxes malformed addresses as PHP `false`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_gethostbyaddr`
- `__rt_getprotobyname`

## Signature summary

```php
function gethostbyaddr(string $ip): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `gethostbyaddr()`](../../../php/builtins/io/gethostbyaddr.md)

