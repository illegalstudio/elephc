---
title: "intlcal_get_greatest_minimum() - internals"
description: "Compiler internals for intlcal_get_greatest_minimum(): lowering path, type checks, and runtime helpers."
---

## `intlcal_get_greatest_minimum()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_intl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_intl.rs)
- **Lowering**: [`src/intl/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/intl/functions.rs) (`intlcal_get_greatest_minimum`)
- **Function symbol**: `intlcal_get_greatest_minimum()`


### Lowering notes

- Implemented by the compiler-injected Intl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function intlcal_get_greatest_minimum(object $calendar, int $field): int|false
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `intlcal_get_greatest_minimum()`](../../../php/builtins/intl/intlcal_get_greatest_minimum.md)
