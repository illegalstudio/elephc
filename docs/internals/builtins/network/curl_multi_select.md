---
title: "curl_multi_select() — internals"
description: "Compiler internals for curl_multi_select(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 377
---

## `curl_multi_select()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_curl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_curl.rs)
- **Lowering**: [`src/curl_prelude.rs`:1582](https://github.com/illegalstudio/elephc/blob/main/src/curl_prelude.rs#L1582) (`curl_multi_select`)
- **Function symbol**: `curl_multi_select()`


### Lowering notes

- Implemented by the compiler-injected curl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function curl_multi_select(CurlMultiHandle $multi_handle, float $timeout = 1.0): int
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_select.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_select.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `curl_multi_select()`](../../../php/builtins/network/curl_multi_select.md)
