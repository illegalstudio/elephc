---
title: "curl_setopt_array()"
description: "Sets multiple options for a cURL transfer."
---

## curl_setopt_array()

```php
function curl_setopt_array(CurlHandle $handle, array $options): bool
```

Sets multiple options for a cURL transfer.

**Parameters**:
- `$handle` (`CurlHandle`)
- `$options` (`array`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_setopt_array` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_setopt_array.md).
