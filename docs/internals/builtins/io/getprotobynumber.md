---
title: "getprotobynumber() — internals"
description: "Compiler internals for getprotobynumber(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 164
---

## `getprotobynumber()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3246](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3246) (`lower_getprotobynumber`)
- **Function symbol**: `lower_getprotobynumber()`


### Lowering notes

- Lowers `getprotobynumber(number)` and boxes a missing entry as PHP `false`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_getprotobynumber`

## Signature summary

```php
function getprotobynumber(int $protocol): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `getprotobynumber()`](../../../php/builtins/io/getprotobynumber.md)

