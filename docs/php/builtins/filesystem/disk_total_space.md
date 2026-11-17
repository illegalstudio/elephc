---
title: "disk_total_space()"
description: "Returns the total size of a filesystem or disk partition."
---

## disk_total_space()

```php
function disk_total_space(string $directory): mixed
```

Returns the total size of a filesystem or disk partition.

**Parameters**:
- `$directory` (`string`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `disk_total_space` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/disk_total_space.md).
