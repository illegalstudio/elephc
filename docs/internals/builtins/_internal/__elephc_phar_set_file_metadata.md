---
title: "__elephc_phar_set_file_metadata() — internals"
description: "Compiler internals for __elephc_phar_set_file_metadata(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 441
---

## `__elephc_phar_set_file_metadata()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`(not lowered)`:0]()
- **Function symbol**: `(none — type-checker only)()`


## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function __elephc_phar_set_file_metadata(mixed $url, mixed $metadata): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
