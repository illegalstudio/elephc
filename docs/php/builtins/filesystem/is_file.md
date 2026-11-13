---
title: "is_file()"
description: "Tells whether the filename is a regular file."
---

## is_file()

```php
function is_file(string $filename): bool
```

Tells whether the filename is a regular file.

**Parameters**:
- `$filename` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `is_file` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/is_file.md).
