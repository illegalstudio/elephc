---
title: "is_link()"
description: "Tells whether the filename is a symbolic link."
---

## is_link()

```php
function is_link(string $filename): bool
```

Tells whether the filename is a symbolic link.

**Parameters**:
- `$filename` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `is_link` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/is_link.md).
