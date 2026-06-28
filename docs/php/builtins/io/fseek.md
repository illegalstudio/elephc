---
title: "fseek()"
description: "Lowers `fseek(stream, offset, whence?)` and clears EOF state on success."
sidebar:
  order: 154
---

## fseek()

```php
function fseek(resource $stream, int $offset, int $whence): int
```

Lowers `fseek(stream, offset, whence?)` and clears EOF state on success.

**Parameters**:
- `$stream` (`resource`)
- `$offset` (`int`)
- `$whence` (`int`), optional

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fseek` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fseek.md).

