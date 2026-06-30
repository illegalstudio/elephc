---
title: "strcasecmp() — internals"
description: "Compiler internals for strcasecmp(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 388
---

## `strcasecmp()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`(not lowered)`:0]()
- **Function symbol**: `(none — type-checker only)()`


## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function strcasecmp(string $string1, string $string2): int
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `strcasecmp()`](../../../php/builtins/string/strcasecmp.md)

