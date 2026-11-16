---
title: "disk_free_space()"
description: "Returns available space on filesystem or disk partition."
---

## disk_free_space()

```php
function disk_free_space(string $directory): mixed
```

Returns available space on filesystem or disk partition.

**Parameters**:
- `$directory` (`string`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `disk_free_space` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/disk_free_space.md).
