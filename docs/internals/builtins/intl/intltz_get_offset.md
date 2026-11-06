---
title: "intltz_get_offset() - internals"
description: "Compiler internals for intltz_get_offset(): lowering path, type checks, and runtime helpers."
---

## `intltz_get_offset()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_intl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_intl.rs)
- **Lowering**: [`src/intl/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/intl/functions.rs) (`intltz_get_offset`)
- **Function symbol**: `intltz_get_offset()`


### Lowering notes

- Implemented by the compiler-injected Intl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function intltz_get_offset(object $timezone, float $timestamp, bool $local, mixed $rawOffset, mixed $dstOffset): bool
```

## What the type checker enforces

- **Arity**: takes exactly 5 arguments.
- **By-reference parameters**: `$rawOffset`, `$dstOffset`.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `intltz_get_offset()`](../../../php/builtins/intl/intltz_get_offset.md)
