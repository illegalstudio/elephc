---
title: "curl_multi_info_read() — internals"
description: "Compiler internals for curl_multi_info_read(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 348
---

## `curl_multi_info_read()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_curl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_curl.rs)
- **Lowering**: [`src/curl_prelude.rs`:1608](https://github.com/illegalstudio/elephc/blob/main/src/curl_prelude.rs#L1608) (`curl_multi_info_read`)
- **Function symbol**: `curl_multi_info_read()`


### Lowering notes

- Implemented by the compiler-injected curl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function curl_multi_info_read(CurlMultiHandle $multi_handle, int $queued_messages = null): mixed
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).
- **By-reference parameters**: `$queued_messages`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_info_read.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_info_read.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `by-reference-or-lvalue`.
- **Dispatch hooks**: `direct`, `values`
- **By-reference parameters**: `$queued_messages`.

## Cross-references

- [User reference for `curl_multi_info_read()`](../../../php/builtins/network/curl_multi_info_read.md)
