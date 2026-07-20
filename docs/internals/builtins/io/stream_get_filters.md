---
title: "stream_get_filters() — internals"
description: "Compiler internals for stream_get_filters(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 219
---

## `stream_get_filters()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_get_filters.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_get_filters.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:1491](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L1491) (`lower_stream_get_filters`)
- **Function symbol**: `lower_stream_get_filters()`


### Lowering notes

- Lowers `stream_get_filters()` to the static built-in filter list.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_get_filters(): array
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/string/stream_get_filters.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/stream_get_filters.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `stream_get_filters()`](../../../php/builtins/io/stream_get_filters.md)
