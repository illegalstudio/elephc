---
title: "curl_multi_get_handles()"
description: "Returns the cURL handles currently attached to a cURL multi handle."
sidebar:
  order: 346
---

## curl_multi_get_handles()

```php
function curl_multi_get_handles(CurlMultiHandle $multi_handle): array
```

Returns the cURL handles currently attached to a cURL multi handle.

**Parameters**:
- `$multi_handle` (`CurlMultiHandle`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected curl prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_get_handles.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/curl/curl_multi_get_handles.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_multi_get_handles` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_multi_get_handles.md).
