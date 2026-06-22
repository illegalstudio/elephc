---
title: "__elephc_phar_get_signature_hash() — internals"
description: "Compiler internals for __elephc_phar_get_signature_hash(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 420
---

## `__elephc_phar_get_signature_hash()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`(not lowered)`:0]()
- **Function symbol**: `(none — type-checker only)()`


## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function __elephc_phar_get_signature_hash(mixed $path): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- _No user-facing reference — this is a compiler internal helper._
