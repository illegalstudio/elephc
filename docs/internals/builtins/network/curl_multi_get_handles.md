---
title: "curl_multi_get_handles() — internals"
description: "Compiler internals for curl_multi_get_handles(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 372
---

## `curl_multi_get_handles()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_curl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_curl.rs)
- **Lowering**: [`src/curl_prelude.rs`:1841](https://github.com/illegalstudio/elephc/blob/main/src/curl_prelude.rs#L1841) (`curl_multi_get_handles`)
- **Function symbol**: `curl_multi_get_handles()`


### Lowering notes

- Implemented by the compiler-injected curl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function curl_multi_get_handles(CurlMultiHandle $multi_handle): array
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_get_handles.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_get_handles.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `curl_multi_get_handles()`](../../../php/builtins/network/curl_multi_get_handles.md)
