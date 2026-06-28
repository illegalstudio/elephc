---
title: "interface_exists() — internals"
description: "Compiler internals for interface_exists(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 64
---

## `interface_exists()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`(not lowered)`:0]()
- **Function symbol**: `(none — type-checker only)()`


## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function interface_exists(string $interface, bool $autoload): bool
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `interface_exists()`](../../../php/builtins/class/interface_exists.md)

