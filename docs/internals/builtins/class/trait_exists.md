---
title: "trait_exists() — internals"
description: "Compiler internals for trait_exists(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 67
---

## `trait_exists()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`(not lowered)`:0]()
- **Function symbol**: `(none — type-checker only)()`


## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function trait_exists(string $trait, bool $autoload): bool
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `trait_exists()`](../../../php/builtins/class/trait_exists.md)

