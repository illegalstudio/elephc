---
title: "get_parent_class() — internals"
description: "Compiler internals for get_parent_class(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 63
---

## `get_parent_class()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`(not lowered)`:0]()
- **Function symbol**: `(none — type-checker only)()`


## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function get_parent_class(mixed $object_or_class): string
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Cross-references

- [User reference for `get_parent_class()`](../../../php/builtins/class/get_parent_class.md)

