---
title: "buffer_new() — internals"
description: "Compiler internals for buffer_new(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 257
---

## `buffer_new()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`(not lowered)`:0]()
- **Function symbol**: `(none — type-checker only)()`


## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function buffer_new(int $length): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `buffer_new()`](../../../php/builtins/misc/buffer_new.md)

