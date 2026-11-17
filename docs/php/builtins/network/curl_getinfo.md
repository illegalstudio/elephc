---
title: "curl_getinfo()"
description: "Gets information about the last transfer."
---

## curl_getinfo()

```php
function curl_getinfo(CurlHandle $handle, ?int $option = null): mixed
```

Gets information about the last transfer.

**Parameters**:
- `$handle` (`CurlHandle`)
- `$option` (`?int`), default `null`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_getinfo` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_getinfo.md).
