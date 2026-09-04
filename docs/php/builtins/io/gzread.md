---
title: "gzread()"
description: "Reads up to length bytes from a gz-file pointer."
sidebar:
  order: 208
---

## gzread()

```php
function gzread(mixed $stream, int $length): mixed
```

Reads up to length bytes from a gz-file pointer.

**Parameters**:
- `$stream` (`mixed`)
- `$length` (`int`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected gz prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gzread` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/gzread.md).
