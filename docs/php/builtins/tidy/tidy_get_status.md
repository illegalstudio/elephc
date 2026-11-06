---
title: "tidy_get_status()"
description: "Returns the document status: 0 for clean, 1 for warnings, 2 for errors."
---

## tidy_get_status()

```php
function tidy_get_status(mixed $tidy): int
```

Returns the document status: 0 for clean, 1 for warnings, 2 for errors.

**Parameters**:
- `$tidy` (`mixed`)

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `tidy_get_status` is implemented in the compiler, see [the internals page](../../../internals/builtins/tidy/tidy_get_status.md).
