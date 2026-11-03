---
title: "basename()"
description: "Returns the trailing name component of a path."
---

## basename()

```php
function basename(string $path, string $suffix = ''): string
```

Returns the trailing name component of a path.

**Parameters**:
- `$path` (`string`)
- `$suffix` (`string`), default `''`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `basename` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/basename.md).
