---
title: "curl_copy_handle()"
description: "Copies a cURL handle along with all of its preferences."
sidebar:
  order: 335
---

## curl_copy_handle()

```php
function curl_copy_handle(CurlHandle $handle): CurlHandle
```

Copies a cURL handle along with all of its preferences.

**Parameters**:
- `$handle` (`CurlHandle`)

**Returns**: `CurlHandle`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_copy_handle.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_copy_handle.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_copy_handle` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_copy_handle.md).
