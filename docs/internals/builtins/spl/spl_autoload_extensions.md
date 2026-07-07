---
title: "spl_autoload_extensions() — internals"
description: "Compiler internals for spl_autoload_extensions(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 321
---

## `spl_autoload_extensions()` — internals

## Where it lives

- **Signature**: [`src/builtins/spl/spl_autoload_extensions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/spl/spl_autoload_extensions.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/spl.rs`:178](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/spl.rs#L178) (`lower_spl_autoload_extensions`)
- **Function symbol**: `lower_spl_autoload_extensions()`


### Lowering notes

- Lowers `spl_autoload_extensions()` against the mutable extension globals.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function spl_autoload_extensions(string $file_extensions = null): string
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Cross-references

- [User reference for `spl_autoload_extensions()`](../../../php/builtins/spl/spl_autoload_extensions.md)
