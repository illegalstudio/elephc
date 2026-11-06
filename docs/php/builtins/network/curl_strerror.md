---
title: "curl_strerror()"
description: "Returns string describing the given error code."
---

## curl_strerror()

```php
function curl_strerror(int $error_code): string
```

Returns string describing the given error code.

**Parameters**:
- `$error_code` (`int`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `curl_strerror` is implemented in the compiler, see [the internals page](../../../internals/builtins/network/curl_strerror.md).
