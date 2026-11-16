---
title: "curl_exec()"
description: "Performs a cURL session."
---

## curl_exec()

```php
function curl_exec(CurlHandle $handle): string|bool
```

Performs a cURL session.

**Parameters**:
- `$handle` (`CurlHandle`)

**Returns**: `string|bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_exec` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_exec.md).
