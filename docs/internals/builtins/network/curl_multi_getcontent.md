---
title: "curl_multi_getcontent() — internals"
description: "Compiler internals for curl_multi_getcontent(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 347
---

## `curl_multi_getcontent()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_curl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_curl.rs)
- **Lowering**: [`src/curl_prelude.rs`:1639](https://github.com/illegalstudio/elephc/blob/main/src/curl_prelude.rs#L1639) (`curl_multi_getcontent`)
- **Function symbol**: `curl_multi_getcontent()`


### Lowering notes

- Implemented by the compiler-injected curl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function curl_multi_getcontent(mixed $handle): ?string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_getcontent.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_getcontent.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `curl_multi_getcontent()`](../../../php/builtins/network/curl_multi_getcontent.md)
