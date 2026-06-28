---
title: "fwrite()"
description: "Lowers `fwrite(stream, data)` and returns the number of bytes written."
sidebar:
  order: 159
---

## fwrite()

```php
function fwrite(resource $stream, string $data, int $length): int
```

Lowers `fwrite(stream, data)` and returns the number of bytes written.

**Parameters**:
- `$stream` (`resource`)
- `$data` (`string`)
- `$length` (`int`)

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fwrite` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fwrite.md).

