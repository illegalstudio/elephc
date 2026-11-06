---
title: "error_reporting()"
description: "Gets or sets the active error reporting mask."
---

## error_reporting()

```php
function error_reporting(mixed $error_level = null): int
```

Gets or sets the active error reporting mask.

**Parameters**:
- `$error_level` (`mixed`), default `null`, optional

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `error_reporting` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/error_reporting.md).
