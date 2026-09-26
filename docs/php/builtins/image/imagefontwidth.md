---
title: "imagefontwidth()"
description: "Returns the pixel width of a built-in font."
sidebar:
  order: 511
---

## imagefontwidth()

```php
function imagefontwidth(int $font): int
```

Returns the pixel width of a built-in font.

**Parameters**:
- `$font` (`int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagefontwidth` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagefontwidth.md).
