---
title: "numfmt_parse_currency() - internals"
description: "Compiler internals for numfmt_parse_currency(): lowering path, type checks, and runtime helpers."
---

## `numfmt_parse_currency()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_intl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_intl.rs)
- **Lowering**: [`src/intl/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/intl/functions.rs) (`numfmt_parse_currency`)
- **Function symbol**: `numfmt_parse_currency()`


### Lowering notes

- Implemented by the compiler-injected Intl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function numfmt_parse_currency(object $formatter, string $string, mixed $currency, int $offset = null): float|false
```

## What the type checker enforces

- **Arity**: takes 3–4 arguments (1 optional).
- **By-reference parameters**: `$currency`, `$offset`.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `numfmt_parse_currency()`](../../../php/builtins/intl/numfmt_parse_currency.md)
