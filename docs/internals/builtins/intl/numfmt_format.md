---
title: "numfmt_format() - internals"
description: "Compiler internals for numfmt_format(): lowering path, type checks, and runtime helpers."
---

## `numfmt_format()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_intl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_intl.rs)
- **Lowering**: [`src/intl/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/intl/functions.rs) (`numfmt_format`)
- **Function symbol**: `numfmt_format()`


### Lowering notes

- Implemented by the compiler-injected Intl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function numfmt_format(object $formatter, int|float $num, int $type = NumberFormatter::TYPE_DEFAULT): string|false
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `numfmt_format()`](../../../php/builtins/intl/numfmt_format.md)
