---
title: "class_exists() — internals"
description: "Compiler internals for class_exists(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 52
---

## `class_exists()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`(not lowered)`:0]()
- **Function symbol**: `(none — type-checker only)()`


## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function class_exists(string $class, bool $autoload): bool
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `class_exists()`](../../../php/builtins/class/class_exists.md)

