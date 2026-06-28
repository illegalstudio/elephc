---
title: "mt_rand() — internals"
description: "Compiler internals for mt_rand(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 242
---

## `mt_rand()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`(not lowered)`:0]()
- **Function symbol**: `(none — type-checker only)()`


## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function mt_rand(int $min, int $max): int
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `mt_rand()`](../../../php/builtins/math/mt_rand.md)

