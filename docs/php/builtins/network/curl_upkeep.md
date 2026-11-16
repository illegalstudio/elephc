---
title: "curl_upkeep()"
description: "Performs any connection upkeep checks."
---

## curl_upkeep()

```php
function curl_upkeep(CurlHandle $handle): bool
```

Performs any connection upkeep checks.

**Parameters**:
- `$handle` (`CurlHandle`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_upkeep` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_upkeep.md).
