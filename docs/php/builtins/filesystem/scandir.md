---
title: "scandir()"
description: "Lists files and directories inside the specified path."
---

## scandir()

```php
function scandir(string $directory, int $sorting_order = 0, mixed $context = null): array
```

Lists files and directories inside the specified path.

**Parameters**:
- `$directory` (`string`)
- `$sorting_order` (`int`), default `0`, optional
- `$context` (`mixed`), default `null`, optional

**Returns**: `array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `scandir` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/scandir.md).
