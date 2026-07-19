---
title: "link() — internals"
description: "Compiler internals for link(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 135
---

## `link()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/link.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/link.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:5453](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L5453) (`lower_link`)
- **Function symbol**: `lower_link()`


### Lowering notes

- Lowers `link(oldpath, newpath)` through the target-aware libc wrapper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_fileatime`
- `__rt_filectime`
- `__rt_link`
- `__rt_readlink`

## Signature summary

```php
function link(string $target, string $link): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/link.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/link.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `link()`](../../../php/builtins/filesystem/link.md)
