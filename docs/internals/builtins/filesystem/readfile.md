---
title: "readfile() — internals"
description: "Compiler internals for readfile(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 137
---

## `readfile()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/readfile.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/readfile.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:300](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L300) (`lower_readfile`)
- **Function symbol**: `lower_readfile()`


### Lowering notes

- Lowers `readfile(path)` and boxes the runtime byte-count-or-false result.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function readfile(string $filename): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `readfile()`](../../../php/builtins/filesystem/readfile.md)

