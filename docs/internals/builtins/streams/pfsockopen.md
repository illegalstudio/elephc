---
title: "pfsockopen() — internals"
description: "Compiler internals for pfsockopen(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 315
---

## `pfsockopen()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`(not lowered)`:0]()
- **Function symbol**: `(none — type-checker only)()`


## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function pfsockopen(string $hostname, int $port, int $error_code, string $error_message, float $timeout): mixed
```

## What the type checker enforces

- **Arity**: takes 2–5 arguments (3 optional).
- **By-reference parameters**: `$error_code`, `$error_message`.

## Cross-references

- [User reference for `pfsockopen()`](../../../php/builtins/streams/pfsockopen.md)

