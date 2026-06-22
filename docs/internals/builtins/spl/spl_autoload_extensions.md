---
title: "spl_autoload_extensions() — internals"
description: "Compiler internals for spl_autoload_extensions(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 309
---

## `spl_autoload_extensions()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/spl.rs`:177](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/spl.rs#L177) (`lower_spl_autoload_extensions`)
- **Function symbol**: `lower_spl_autoload_extensions()`


### Lowering notes

- Lowers `spl_autoload_extensions()` against the legacy mutable extension globals.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function spl_autoload_extensions(string $file_extensions): string
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Cross-references

- [User reference for `spl_autoload_extensions()`](../../../php/builtins/spl/spl_autoload_extensions.md)

