---
title: "curl_multi_remove_handle()"
description: "Removes a multi handle from a set of cURL handles."
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

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_multi_remove_handle` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_multi_remove_handle.md).
