---
title: "tempnam()"
description: "Creates a file with a unique filename."
---

## tempnam()

```php
function tempnam(string $directory, string $prefix): string
```

Creates a file with a unique filename.

**Parameters**:
- `$directory` (`string`)
- `$prefix` (`string`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `tempnam` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/tempnam.md).
