---
title: "md5() — internals"
description: "Compiler internals for md5(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 352
---

## `md5()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/strings.rs`:355](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/strings.rs#L355) (`lower_md5`)
- **Function symbol**: `lower_md5()`


### Lowering notes

- Lowers `md5(data, binary?)` through the shared crypto-backed runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_hash`
- `__rt_md5`
- `__rt_sha1`

## Signature summary

```php
function md5(string $string, bool $binary): string
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `md5()`](../../../php/builtins/string/md5.md)

