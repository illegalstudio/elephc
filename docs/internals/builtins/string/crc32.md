---
title: "crc32() — internals"
description: "Compiler internals for crc32(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 343
---

## `crc32()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:348](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L348) (`lower_crc32`)
- **Function symbol**: `lower_crc32()`


### Lowering notes

- Lowers `crc32(string)` through the shared checksum runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_crc32`
- `__rt_hash`
- `__rt_md5`
- `__rt_sha1`

## Signature summary

```php
function crc32(string $string): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `crc32()`](../../../php/builtins/string/crc32.md)

