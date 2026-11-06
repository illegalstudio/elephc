---
title: "fread()"
description: "Binary-safe file read."
---

## fread()

```php
function fread(resource $stream, int $length): string
```

Binary-safe file read.

**Parameters**:
- `$stream` (`resource`)
- `$length` (`int`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `fread` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fread.md).
