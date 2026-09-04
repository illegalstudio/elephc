---
title: "gzwrite()"
description: "Writes a string to a gz-file pointer."
sidebar:
  order: 212
---

## gzwrite()

```php
function gzwrite(mixed $stream, string $data, int $length = null): mixed
```

Writes a string to a gz-file pointer.

**Parameters**:
- `$stream` (`mixed`)
- `$data` (`string`)
- `$length` (`int`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected gz prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gzwrite` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/gzwrite.md).
