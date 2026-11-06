---
title: "curl_setopt()"
description: "Sets an option for a cURL transfer."
---

## curl_setopt()

```php
function curl_setopt(CurlHandle $handle, int $option, mixed $value): bool
```

Sets an option for a cURL transfer.

**Parameters**:
- `$handle` (`CurlHandle`)
- `$option` (`int`)
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_setopt` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_setopt.md).
