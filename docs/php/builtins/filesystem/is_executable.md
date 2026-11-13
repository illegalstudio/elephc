---
title: "is_executable()"
description: "Tells whether the filename is executable."
---

## is_executable()

```php
function is_executable(string $filename): bool
```

Tells whether the filename is executable.

**Parameters**:
- `$filename` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `is_executable` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/is_executable.md).
