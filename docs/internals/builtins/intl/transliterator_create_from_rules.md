---
title: "transliterator_create_from_rules() - internals"
description: "Compiler internals for transliterator_create_from_rules(): lowering path, type checks, and runtime helpers."
---

## `transliterator_create_from_rules()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_intl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_intl.rs)
- **Lowering**: [`src/intl/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/intl/functions.rs) (`transliterator_create_from_rules`)
- **Function symbol**: `transliterator_create_from_rules()`


### Lowering notes

- Implemented by the compiler-injected Intl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function transliterator_create_from_rules(string $rules, int $direction = Transliterator::FORWARD): ?object
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `transliterator_create_from_rules()`](../../../php/builtins/intl/transliterator_create_from_rules.md)
