---
title: "curl_error()"
description: "Returns a string describing the last cURL error."
---

## curl_error()

```php
function curl_error(CurlHandle $handle): string
```

Returns a string describing the last cURL error.

**Parameters**:
- `$handle` (`CurlHandle`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_error` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_error.md).
