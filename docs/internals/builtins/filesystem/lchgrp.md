---
title: "lchgrp() — internals"
description: "Compiler internals for lchgrp(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 133
---

## `lchgrp()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/lchgrp.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/lchgrp.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:4484](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L4484) (`lower_lchgrp`)
- **Function symbol**: `lower_lchgrp()`


### Lowering notes

- Lowers `lchgrp(path, group)` for integer GIDs and string group names without following symlinks.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_umask`

## Signature summary

```php
function lchgrp(string $filename, string $group): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/lchgrp.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/lchgrp.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `lchgrp()`](../../../php/builtins/filesystem/lchgrp.md)
