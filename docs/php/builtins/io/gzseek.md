---
title: "gzseek()"
description: "Seeks on a gz-file pointer."
sidebar:
  order: 210
---

## gzseek()

```php
function gzseek(mixed $stream, int $offset, int $whence = 0): int
```

Seeks on a gz-file pointer.

**Parameters**:
- `$stream` (`mixed`)
- `$offset` (`int`)
- `$whence` (`int`), default `0`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected gz prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gzseek` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/gzseek.md).
