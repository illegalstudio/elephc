---
title: "imagegif()"
description: "Writes an image as GIF, to a file or to the output buffer."
sidebar:
  order: 514
---

## imagegif()

```php
function imagegif(mixed $image, ?string $file = null): bool
```

Writes an image as GIF, to a file or to the output buffer.

**Parameters**:
- `$image` (`mixed`)
- `$file` (`?string`), default `null`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagegif` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagegif.md).
