---
title: "curl_init()"
description: "Initializes a cURL session."
---

## curl_init()

```php
function curl_init(?string $url = null): CurlHandle
```

Initializes a cURL session.

**Parameters**:
- `$url` (`?string`), default `null`, optional

**Returns**: `CurlHandle`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_init` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_init.md).
