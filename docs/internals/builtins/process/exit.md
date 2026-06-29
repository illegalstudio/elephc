---
title: "exit() — internals"
description: "Compiler internals for exit(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 290
---

## `exit()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`(not lowered)`:0]()
- **Function symbol**: `(none — type-checker only)()`


## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function exit(int $status): void
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Cross-references

- [User reference for `exit()`](../../../php/builtins/process/exit.md)

