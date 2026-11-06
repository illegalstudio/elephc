---
title: "scandir()"
description: "Lists files and directories inside the specified path."
---

## scandir()

```php
function scandir(string $directory): array
```

Lists files and directories inside the specified path.

**Parameters**:
- `$directory` (`string`)

**Returns**: `array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `scandir` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/scandir.md).
