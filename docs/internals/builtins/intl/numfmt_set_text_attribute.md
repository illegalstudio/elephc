---
title: "numfmt_set_text_attribute() - internals"
description: "Compiler internals for numfmt_set_text_attribute(): lowering path, type checks, and runtime helpers."
---

## `numfmt_set_text_attribute()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_intl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_intl.rs)
- **Lowering**: [`src/intl/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/intl/functions.rs) (`numfmt_set_text_attribute`)
- **Function symbol**: `numfmt_set_text_attribute()`


### Lowering notes

- Implemented by the compiler-injected Intl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function numfmt_set_text_attribute(object $formatter, int $attribute, string $value): bool
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `numfmt_set_text_attribute()`](../../../php/builtins/intl/numfmt_set_text_attribute.md)
