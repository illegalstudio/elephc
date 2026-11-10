---
title: "datefmt_set_lenient() - internals"
description: "Compiler internals for datefmt_set_lenient(): lowering path, type checks, and runtime helpers."
---

## `datefmt_set_lenient()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_intl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_intl.rs)
- **Lowering**: [`src/intl/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/intl/functions.rs) (`datefmt_set_lenient`)
- **Function symbol**: `datefmt_set_lenient()`


### Lowering notes

- Implemented by the compiler-injected Intl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function datefmt_set_lenient(object $formatter, bool $lenient): void
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `datefmt_set_lenient()`](../../../php/builtins/intl/datefmt_set_lenient.md)
