---
title: "lchgrp() — internals"
description: "Compiler internals for lchgrp(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 112
---

## `lchgrp()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3845](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3845) (`lower_lchgrp`)
- **Function symbol**: `lower_lchgrp()`


### Lowering notes

- Lowers `lchgrp(path, group)` for integer GIDs and string group names without following symlinks.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_umask`

## Signature summary

```php
function lchgrp(string $filename, int $group): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `lchgrp()`](../../../php/builtins/filesystem/lchgrp.md)

