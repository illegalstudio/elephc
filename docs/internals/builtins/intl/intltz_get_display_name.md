---
title: "intltz_get_display_name() - internals"
description: "Compiler internals for intltz_get_display_name(): lowering path, type checks, and runtime helpers."
---

## `intltz_get_display_name()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_intl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_intl.rs)
- **Lowering**: [`src/intl/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/intl/functions.rs) (`intltz_get_display_name`)
- **Function symbol**: `intltz_get_display_name()`


### Lowering notes

- Implemented by the compiler-injected Intl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function intltz_get_display_name(object $timezone, bool $dst = false, int $style = IntlTimeZone::DISPLAY_LONG, ?string $locale = null): string|false
```

## What the type checker enforces

- **Arity**: takes 1–4 arguments (3 optional).

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `intltz_get_display_name()`](../../../php/builtins/intl/intltz_get_display_name.md)
