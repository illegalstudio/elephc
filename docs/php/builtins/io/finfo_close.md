---
title: "finfo_close()"
description: "Returns true; PHP 8 finfo instances are freed automatically. Deprecated since PHP 8.5."
---

## finfo_close()

```php
function finfo_close(finfo $finfo): bool
```

Returns true; PHP 8 finfo instances are freed automatically. Deprecated since PHP 8.5.

**Parameters**:
- `$finfo` (`finfo`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `finfo_close` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/finfo_close.md).
