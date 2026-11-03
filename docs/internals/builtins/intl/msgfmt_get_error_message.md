---
title: "msgfmt_get_error_message() - internals"
description: "Compiler internals for msgfmt_get_error_message(): lowering path, type checks, and runtime helpers."
---

## `msgfmt_get_error_message()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_intl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_intl.rs)
- **Lowering**: [`src/intl/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/intl/functions.rs) (`msgfmt_get_error_message`)
- **Function symbol**: `msgfmt_get_error_message()`


### Lowering notes

- Implemented by the compiler-injected Intl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function msgfmt_get_error_message(object $formatter): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `msgfmt_get_error_message()`](../../../php/builtins/intl/msgfmt_get_error_message.md)
