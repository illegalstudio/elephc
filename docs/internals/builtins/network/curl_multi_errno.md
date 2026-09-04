---
title: "curl_multi_errno() — internals"
description: "Compiler internals for curl_multi_errno(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 370
---

## `curl_multi_errno()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_curl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_curl.rs)
- **Lowering**: [`src/curl_prelude.rs`:1714](https://github.com/illegalstudio/elephc/blob/main/src/curl_prelude.rs#L1714) (`curl_multi_errno`)
- **Function symbol**: `curl_multi_errno()`


### Lowering notes

- Implemented by the compiler-injected curl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function curl_multi_errno(CurlMultiHandle $multi_handle): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_errno.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_errno.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `curl_multi_errno()`](../../../php/builtins/network/curl_multi_errno.md)
