---
title: "is_readable()"
description: "Tells whether the filename is readable."
---

## is_readable()

```php
function is_readable(string $filename): bool
```

Tells whether the filename is readable.

**Parameters**:
- `$filename` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `is_readable` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/is_readable.md).
