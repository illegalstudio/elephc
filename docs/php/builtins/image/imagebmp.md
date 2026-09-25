---
title: "imagebmp()"
description: "Writes an image as BMP, to a file or to the output buffer."
sidebar:
  order: 465
---

## imagebmp()

```php
function imagebmp(mixed $image, ?string $file = null, bool $compressed = true): bool
```

Writes an image as BMP, to a file or to the output buffer.

**Parameters**:
- `$image` (`mixed`)
- `$file` (`?string`), default `null`, optional
- `$compressed` (`bool`), default `true`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagebmp` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagebmp.md).
