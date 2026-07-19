---
title: "readfile() — internals"
description: "Compiler internals for readfile(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 141
---

## `readfile()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/readfile.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/readfile.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:300](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L300) (`lower_readfile`)
- **Function symbol**: `lower_readfile()`


### Lowering notes

- Lowers `readfile(path)` and boxes the runtime byte-count-or-false result.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_vd_write`

## Signature summary

```php
function readfile(string $filename): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/readfile.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/readfile.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `readfile()`](../../../php/builtins/filesystem/readfile.md)
