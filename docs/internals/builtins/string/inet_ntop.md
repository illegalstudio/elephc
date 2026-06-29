---
title: "inet_ntop() — internals"
description: "Compiler internals for inet_ntop(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 348
---

## `inet_ntop()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:497](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L497) (`lower_inet`)
- **Function symbol**: `lower_inet()`


### Lowering notes

- Lowers `inet_ntop()` and `inet_pton()` and boxes invalid-address results as PHP false.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_sprintf`

## Signature summary

```php
function inet_ntop(string $ip): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `inet_ntop()`](../../../php/builtins/string/inet_ntop.md)

