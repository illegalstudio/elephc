---
title: "opendir()"
description: "Open directory handle."
---

## opendir()

```php
function opendir(string $directory): resource|false
```

Open directory handle.

**Parameters**:
- `$directory` (`string`)

**Returns**: `resource|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `opendir` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/opendir.md).
