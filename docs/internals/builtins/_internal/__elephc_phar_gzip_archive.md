---
title: "__elephc_phar_gzip_archive() — internals"
description: "Compiler internals for __elephc_phar_gzip_archive(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 438
---

## `__elephc_phar_gzip_archive()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:4105](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L4105) (`lower_elephc_phar_gzip_archive`)
- **Function symbol**: `lower_elephc_phar_gzip_archive()`


### Lowering notes

- Lowers `__elephc_phar_gzip_archive(src)` into the whole-archive gzip bridge,
- returning the written destination path (or an empty string on failure).

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function __elephc_phar_gzip_archive(mixed $src): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
