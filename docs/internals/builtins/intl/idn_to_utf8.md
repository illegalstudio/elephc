---
title: "idn_to_utf8() - internals"
description: "Compiler internals for idn_to_utf8(): lowering path, type checks, and runtime helpers."
---

## `idn_to_utf8()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_intl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_intl.rs)
- **Lowering**: [`src/intl/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/intl/functions.rs) (`idn_to_utf8`)
- **Function symbol**: `idn_to_utf8()`


### Lowering notes

- Implemented by the compiler-injected Intl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function idn_to_utf8(string $domain, int $flags = IDNA_DEFAULT, int $variant = INTL_IDNA_VARIANT_UTS46, mixed $idna_info = null): string|false
```

## What the type checker enforces

- **Arity**: takes 1–4 arguments (3 optional).
- **By-reference parameters**: `$idna_info`.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `idn_to_utf8()`](../../../php/builtins/intl/idn_to_utf8.md)
