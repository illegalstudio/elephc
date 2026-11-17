---
title: "curl_unescape()"
description: "Decodes the given URL-encoded string."
---

## curl_unescape()

```php
function curl_unescape(CurlHandle $handle, string $string): string
```

Decodes the given URL-encoded string.

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

For how `curl_unescape` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_unescape.md).
