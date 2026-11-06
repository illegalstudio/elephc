---
title: "dirname()"
description: "Returns a parent directory's path."
---

## dirname()

```php
function dirname(string $path, int $levels = 1): string
```

Returns a parent directory's path.

**Parameters**:
- `$path` (`string`)
- `$levels` (`int`), default `1`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `dirname` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/dirname.md).
