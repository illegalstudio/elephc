---
title: "stream_get_wrappers() — internals"
description: "Compiler internals for stream_get_wrappers(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 223
---

## `stream_get_wrappers()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_get_wrappers.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_get_wrappers.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:1460](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L1460) (`lower_stream_get_wrappers`)
- **Function symbol**: `lower_stream_get_wrappers()`


### Lowering notes

- Lowers `stream_get_wrappers()` to the static built-in wrapper list.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_get_wrappers(): array
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/stream_get_wrappers.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/stream_get_wrappers.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `stream_get_wrappers()`](../../../php/builtins/io/stream_get_wrappers.md)
