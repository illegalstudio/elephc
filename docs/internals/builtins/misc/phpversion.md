---
title: "phpversion() — internals"
description: "Compiler internals for phpversion(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 282
---

## `phpversion()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins.rs`:776](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins.rs#L776) (`lower_phpversion`)
- **Function symbol**: `lower_phpversion()`


### Lowering notes

- Lowers `phpversion()` as the compiler package version string.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function phpversion(string $extension = null): string
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Cross-references

- [User reference for `phpversion()`](../../../php/builtins/misc/phpversion.md)

