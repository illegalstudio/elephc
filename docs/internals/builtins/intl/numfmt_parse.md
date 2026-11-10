---
title: "numfmt_parse() - internals"
description: "Compiler internals for numfmt_parse(): lowering path, type checks, and runtime helpers."
---

## `numfmt_parse()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_intl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_intl.rs)
- **Lowering**: [`src/intl/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/intl/functions.rs) (`numfmt_parse`)
- **Function symbol**: `numfmt_parse()`


### Lowering notes

- Implemented by the compiler-injected Intl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function numfmt_parse(object $formatter, string $string, int $type = NumberFormatter::TYPE_DOUBLE, int $offset = null): int|float|false
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).
- **By-reference parameters**: `$offset`.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `numfmt_parse()`](../../../php/builtins/intl/numfmt_parse.md)
