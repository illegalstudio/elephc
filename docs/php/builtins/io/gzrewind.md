---
title: "gzrewind()"
description: "Rewinds the position of a gz-file pointer."
sidebar:
  order: 209
---

## gzrewind()

```php
function gzrewind(mixed $stream): bool
```

Rewinds the position of a gz-file pointer.

**Parameters**:
- `$stream` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected gz prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gzrewind` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/gzrewind.md).
