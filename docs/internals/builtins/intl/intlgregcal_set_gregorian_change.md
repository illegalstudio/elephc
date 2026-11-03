---
title: "intlgregcal_set_gregorian_change() - internals"
description: "Compiler internals for intlgregcal_set_gregorian_change(): lowering path, type checks, and runtime helpers."
---

## `intlgregcal_set_gregorian_change()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_intl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_intl.rs)
- **Lowering**: [`src/intl/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/intl/functions.rs) (`intlgregcal_set_gregorian_change`)
- **Function symbol**: `intlgregcal_set_gregorian_change()`


### Lowering notes

- Implemented by the compiler-injected Intl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function intlgregcal_set_gregorian_change(object $calendar, float $timestamp): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `intlgregcal_set_gregorian_change()`](../../../php/builtins/intl/intlgregcal_set_gregorian_change.md)
