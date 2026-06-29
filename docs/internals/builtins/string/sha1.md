---
title: "sha1() — internals"
description: "Compiler internals for sha1(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 362
---

## `sha1()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:360](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L360) (`lower_sha1`)
- **Function symbol**: `lower_sha1()`


### Lowering notes

- Lowers `sha1(data, binary?)` through the shared crypto-backed runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_hash`
- `__rt_sha1`

## Signature summary

```php
function sha1(string $string, bool $binary): string
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `sha1()`](../../../php/builtins/string/sha1.md)

