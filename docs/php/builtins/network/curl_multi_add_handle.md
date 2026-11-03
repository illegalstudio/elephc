---
title: "curl_multi_add_handle()"
description: "Adds a normal cURL handle to a cURL multi handle."
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

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_multi_add_handle` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_multi_add_handle.md).
