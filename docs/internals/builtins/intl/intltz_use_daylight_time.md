---
title: "intltz_use_daylight_time() - internals"
description: "Compiler internals for intltz_use_daylight_time(): lowering path, type checks, and runtime helpers."
---

## `intltz_use_daylight_time()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_intl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_intl.rs)
- **Lowering**: [`src/intl/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/intl/functions.rs) (`intltz_use_daylight_time`)
- **Function symbol**: `intltz_use_daylight_time()`


### Lowering notes

- Implemented by the compiler-injected Intl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function intltz_use_daylight_time(object $timezone): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `intltz_use_daylight_time()`](../../../php/builtins/intl/intltz_use_daylight_time.md)
