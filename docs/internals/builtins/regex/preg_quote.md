---
title: "preg_quote() - internals"
description: "Compiler internals for preg_quote(): lowering path, type checks, and runtime helpers."
---

## `preg_quote()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/pcre_prelude.rs`](https://github.com/illegalstudio/elephc/blob/main/src/pcre_prelude.rs) (`preg_quote`)
- **Function symbol**: `preg_quote()`


### Lowering notes

- Implemented by the compiler-injected PCRE prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function preg_quote(string $str, ?string $delimiter = null): string
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/regex/preg_quote.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/regex/preg_quote.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `preg_quote()`](../../../php/builtins/regex/preg_quote.md)
