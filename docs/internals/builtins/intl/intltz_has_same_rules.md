---
title: "intltz_has_same_rules() - internals"
description: "Compiler internals for intltz_has_same_rules(): lowering path, type checks, and runtime helpers."
---

## `intltz_has_same_rules()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_intl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_intl.rs)
- **Lowering**: [`src/intl/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/intl/functions.rs) (`intltz_has_same_rules`)
- **Function symbol**: `intltz_has_same_rules()`


### Lowering notes

- Implemented by the compiler-injected Intl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function intltz_has_same_rules(object $timezone, object $other): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `intltz_has_same_rules()`](../../../php/builtins/intl/intltz_has_same_rules.md)
