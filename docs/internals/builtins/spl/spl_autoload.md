---
title: "spl_autoload() — internals"
description: "Compiler internals for spl_autoload(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 322
---

## `spl_autoload()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/spl.rs`:150](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/spl.rs#L150) (`lower_spl_autoload_void`)
- **Function symbol**: `lower_spl_autoload_void()`


### Lowering notes

- Lowers no-op autoload calls by preserving arg effects and returning PHP null if used.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function spl_autoload(string $class, string $file_extensions): void
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `spl_autoload()`](../../../php/builtins/spl/spl_autoload.md)

