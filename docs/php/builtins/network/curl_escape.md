---
title: "curl_escape()"
description: "URL-encodes a string with the given cURL handle."
---

## curl_escape()

```php
function curl_escape(CurlHandle $handle, string $string): string
```

URL-encodes a string with the given cURL handle.

**Parameters**:
- `$handle` (`CurlHandle`)
- `$string` (`string`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_escape` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_escape.md).
