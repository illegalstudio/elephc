---
title: "preg_replace_callback_array() - internals"
description: "Compiler internals for preg_replace_callback_array(): lowering path, type checks, and runtime helpers."
---

## `preg_replace_callback_array()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/pcre_prelude.rs`](https://github.com/illegalstudio/elephc/blob/main/src/pcre_prelude.rs) (`preg_replace_callback_array`)
- **Function symbol**: `preg_replace_callback_array()`


### Lowering notes

- Implemented by the compiler-injected PCRE prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function preg_replace_callback_array(array $pattern, string $subject): ?string
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `preg_replace_callback_array()`](../../../php/builtins/regex/preg_replace_callback_array.md)
