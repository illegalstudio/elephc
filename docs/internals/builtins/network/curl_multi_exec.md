---
title: "curl_multi_exec() — internals"
description: "Compiler internals for curl_multi_exec(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 345
---

## `curl_multi_exec()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_curl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_curl.rs)
- **Lowering**: [`src/curl_prelude.rs`:1585](https://github.com/illegalstudio/elephc/blob/main/src/curl_prelude.rs#L1585) (`curl_multi_exec`)
- **Function symbol**: `curl_multi_exec()`


### Lowering notes

- Implemented by the compiler-injected curl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function curl_multi_exec(CurlMultiHandle $multi_handle, int $still_running): int
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.
- **By-reference parameters**: `$still_running`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_exec.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_exec.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `by-reference-or-lvalue`.
- **Dispatch hooks**: `direct`, `values`
- **By-reference parameters**: `$still_running`.

## Cross-references

- [User reference for `curl_multi_exec()`](../../../php/builtins/network/curl_multi_exec.md)
