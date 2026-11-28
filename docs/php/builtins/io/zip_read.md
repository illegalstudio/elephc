---
title: "zip_read()"
description: "Returns the next entry resource in an open zip directory, or false at the end."
---

## zip_read()

```php
function zip_read(mixed $zip): mixed
```

Returns the next entry resource in an open zip directory, or false at the end.

**Parameters**:
- `$zip` (`mixed`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Not available yet |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `zip_read` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/zip_read.md).
