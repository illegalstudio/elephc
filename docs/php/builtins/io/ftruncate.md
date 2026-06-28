---
title: "ftruncate()"
description: "Lowers `ftruncate(stream, size)` through the shared fd truncate runtime helper."
sidebar:
  order: 158
---

## ftruncate()

```php
function ftruncate(resource $stream, int $size): bool
```

Lowers `ftruncate(stream, size)` through the shared fd truncate runtime helper.

**Parameters**:
- `$stream` (`resource`)
- `$size` (`int`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ftruncate` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/ftruncate.md).

