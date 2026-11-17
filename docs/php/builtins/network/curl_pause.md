---
title: "curl_pause()"
description: "Pauses and unpauses a connection."
---

## curl_pause()

```php
function curl_pause(CurlHandle $handle, int $flags): int
```

Pauses and unpauses a connection.

**Parameters**:
- `$handle` (`CurlHandle`)
- `$flags` (`int`)

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_pause` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_pause.md).
