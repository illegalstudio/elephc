---
title: "str_ireplace() — internals"
description: "Compiler internals for str_ireplace(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 365
---

## `str_ireplace()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`(not lowered)`:0]()
- **Function symbol**: `(none — type-checker only)()`


## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function str_ireplace(mixed $search, mixed $replace, mixed $subject, int $count): mixed
```

## What the type checker enforces

- **Arity**: takes 3–4 arguments (1 optional).
- **By-reference parameters**: `$count`.

## Cross-references

- [User reference for `str_ireplace()`](../../../php/builtins/string/str_ireplace.md)

