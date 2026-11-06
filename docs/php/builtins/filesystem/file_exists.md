---
title: "file_exists()"
description: "Checks whether a file or directory exists."
---

## file_exists()

```php
function file_exists(string $filename): bool
```

Checks whether a file or directory exists.

**Parameters**:
- `$filename` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `file_exists` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/file_exists.md).
