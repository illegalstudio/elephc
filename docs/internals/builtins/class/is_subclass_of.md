---
title: "is_subclass_of() — internals"
description: "Compiler internals for is_subclass_of(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 66
---

## `is_subclass_of()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`(not lowered)`:0]()
- **Function symbol**: `(none — type-checker only)()`


## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function is_subclass_of(mixed $object_or_class, string $class, bool $allow_string): bool
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Cross-references

- [User reference for `is_subclass_of()`](../../../php/builtins/class/is_subclass_of.md)

