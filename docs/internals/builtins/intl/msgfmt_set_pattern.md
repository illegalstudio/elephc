---
title: "msgfmt_set_pattern() - internals"
description: "Compiler internals for msgfmt_set_pattern(): lowering path, type checks, and runtime helpers."
---

## `msgfmt_set_pattern()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_intl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_intl.rs)
- **Lowering**: [`src/intl/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/intl/functions.rs) (`msgfmt_set_pattern`)
- **Function symbol**: `msgfmt_set_pattern()`


### Lowering notes

- Implemented by the compiler-injected Intl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function msgfmt_set_pattern(object $formatter, string $pattern): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `msgfmt_set_pattern()`](../../../php/builtins/intl/msgfmt_set_pattern.md)
