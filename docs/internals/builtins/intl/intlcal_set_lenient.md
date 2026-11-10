---
title: "intlcal_set_lenient() - internals"
description: "Compiler internals for intlcal_set_lenient(): lowering path, type checks, and runtime helpers."
---

## `intlcal_set_lenient()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_intl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_intl.rs)
- **Lowering**: [`src/intl/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/intl/functions.rs) (`intlcal_set_lenient`)
- **Function symbol**: `intlcal_set_lenient()`


### Lowering notes

- Implemented by the compiler-injected Intl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function intlcal_set_lenient(object $calendar, bool $lenient): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `intlcal_set_lenient()`](../../../php/builtins/intl/intlcal_set_lenient.md)
