---
title: "tmpfile() — internals"
description: "Compiler internals for tmpfile(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 153
---

## `tmpfile()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/tmpfile.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/tmpfile.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:5417](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L5417) (`lower_tmpfile`)
- **Function symbol**: `lower_tmpfile()`


### Lowering notes

- Lowers `tmpfile()` and boxes the anonymous stream descriptor or PHP false.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_filemtime`
- `__rt_linkinfo`
- `__rt_tmpfile`

## Signature summary

```php
function tmpfile(): mixed
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/tmpfile.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/tmpfile.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `tmpfile()`](../../../php/builtins/filesystem/tmpfile.md)
