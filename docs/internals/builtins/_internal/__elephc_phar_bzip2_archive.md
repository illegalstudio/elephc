---
title: "__elephc_phar_bzip2_archive() — internals"
description: "Compiler internals for __elephc_phar_bzip2_archive(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 430
---

## `__elephc_phar_bzip2_archive()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/__elephc_phar_bzip2_archive.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/__elephc_phar_bzip2_archive.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:4120](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L4120) (`lower_elephc_phar_bzip2_archive`)
- **Function symbol**: `lower_elephc_phar_bzip2_archive()`


### Lowering notes

- Lowers `__elephc_phar_bzip2_archive(src)` into the whole-archive bzip2 bridge,
- returning the written destination path (or an empty string on failure).

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function __elephc_phar_bzip2_archive(string $src): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
