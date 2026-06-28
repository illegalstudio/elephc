---
title: "call_user_func() — internals"
description: "Compiler internals for call_user_func(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 258
---

## `call_user_func()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`(not lowered)`:0]()
- **Function symbol**: `(none — type-checker only)()`


## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function call_user_func(callable $callback, ...$args): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **Variadic**: collects excess arguments into `$args`.

## Cross-references

- [User reference for `call_user_func()`](../../../php/builtins/misc/call_user_func.md)

