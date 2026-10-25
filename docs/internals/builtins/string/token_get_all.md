---
title: "token_get_all() - internals"
description: "Compiler internals for token_get_all(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 944
---

## `token_get_all()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/tokenizer_prelude.rs`:48](https://github.com/illegalstudio/elephc/blob/main/src/tokenizer_prelude.rs#L48) (`token_get_all`)
- **Function symbol**: `token_get_all()`


### Lowering notes

- Implemented by the compiler-injected tokenizer prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function token_get_all(string $code, int $flags = 0): array
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/tokenizer/token_get_all.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/tokenizer/token_get_all.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `token_get_all()`](../../../php/builtins/string/token_get_all.md)
