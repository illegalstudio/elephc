---
title: "curl_exec() — internals"
description: "Compiler internals for curl_exec(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 339
---

## `curl_exec()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_curl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_curl.rs)
- **Lowering**: [`src/curl_prelude.rs`:1129](https://github.com/illegalstudio/elephc/blob/main/src/curl_prelude.rs#L1129) (`curl_exec`)
- **Function symbol**: `curl_exec()`


### Lowering notes

- Implemented by the compiler-injected curl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function curl_exec(CurlHandle $handle): string|bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/curl/curl_exec.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_exec.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `dynamic-language-surface`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `curl_exec()`](../../../php/builtins/network/curl_exec.md)
