---
title: "curl_multi_remove_handle()"
description: "Removes a multi handle from a set of cURL handles."
sidebar:
  order: 376
---

## curl_multi_remove_handle()

```php
function curl_multi_remove_handle(CurlMultiHandle $multi_handle, mixed $handle): int
```

Removes a multi handle from a set of cURL handles.

**Parameters**:
- `$multi_handle` (`CurlMultiHandle`)
- `$handle` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_remove_handle.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_remove_handle.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_multi_remove_handle` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_multi_remove_handle.md).
