---
title: "class_uses() — internals"
description: "Compiler internals for class_uses(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 56
---

## `class_uses()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`(not lowered)`:0]()
- **Function symbol**: `(none — type-checker only)()`


## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function class_uses(mixed $object_or_class, bool $autoload): mixed
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `class_uses()`](../../../php/builtins/class/class_uses.md)

