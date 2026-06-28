---
title: "call_user_func_array() — internals"
description: "Compiler internals for call_user_func_array(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 259
---

## `call_user_func_array()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`(not lowered)`:0]()
- **Function symbol**: `(none — type-checker only)()`


## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function call_user_func_array(callable $callback, array $args): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `call_user_func_array()`](../../../php/builtins/misc/call_user_func_array.md)

