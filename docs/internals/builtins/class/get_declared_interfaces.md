---
title: "get_declared_interfaces() — internals"
description: "Compiler internals for get_declared_interfaces(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 61
---

## `get_declared_interfaces()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`(not lowered)`:0]()
- **Function symbol**: `(none — type-checker only)()`


## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function get_declared_interfaces(): array
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Cross-references

- [User reference for `get_declared_interfaces()`](../../../php/builtins/class/get_declared_interfaces.md)

