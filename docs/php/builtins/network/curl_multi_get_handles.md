---
title: "curl_multi_get_handles()"
description: "Returns the cURL handles currently attached to a cURL multi handle."
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

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_multi_get_handles` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_multi_get_handles.md).
