---
title: "is_writable()"
description: "Tells whether the filename is writable."
---

## is_writable()

```php
function is_writable(string $filename): bool
```

Tells whether the filename is writable.

**Parameters**:
- `$filename` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `is_writable` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/is_writable.md).
