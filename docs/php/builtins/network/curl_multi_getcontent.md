---
title: "curl_multi_getcontent()"
description: "Returns the content of a cURL handle if CURLOPT_RETURNTRANSFER is set."
---

## curl_multi_getcontent()

```php
function curl_multi_getcontent(mixed $handle): ?string
```

Returns the content of a cURL handle if CURLOPT_RETURNTRANSFER is set.

**Parameters**:
- `$handle` (`mixed`)

**Returns**: `?string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_multi_getcontent` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_multi_getcontent.md).
