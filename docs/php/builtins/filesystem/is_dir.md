---
title: "is_dir()"
description: "Tells whether the filename is a directory."
---

## is_dir()

```php
function is_dir(string $filename): bool
```

Tells whether the filename is a directory.

**Parameters**:
- `$filename` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `is_dir` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/is_dir.md).
