---
title: "curl_multi_add_handle()"
description: "Adds a normal cURL handle to a cURL multi handle."
sidebar:
  order: 368
---

## curl_multi_add_handle()

```php
function curl_multi_add_handle(CurlMultiHandle $multi_handle, mixed $handle): int
```

Adds a normal cURL handle to a cURL multi handle.

**Parameters**:
- `$multi_handle` (`CurlMultiHandle`)
- `$handle` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_add_handle.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_add_handle.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_multi_add_handle` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_multi_add_handle.md).
