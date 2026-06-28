---
title: "flock()"
description: "Lowers `flock(stream, operation, would_block?)` through the libc flock wrapper."
sidebar:
  order: 147
---

## flock()

```php
function flock(resource $stream, int $operation, bool $would_block): bool
```

Lowers `flock(stream, operation, would_block?)` through the libc flock wrapper.

**Parameters**:
- `$stream` (`resource`)
- `$operation` (`int`)
- `$would_block` (`bool`), passed by reference, optional

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `flock` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/flock.md).

