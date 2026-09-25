---
title: "imagecreatetruecolor()"
description: "Creates an empty truecolor image of the given size."
sidebar:
  order: 498
---

## imagecreatetruecolor()

```php
function imagecreatetruecolor(int $width, int $height): mixed
```

Creates an empty truecolor image of the given size.

**Parameters**:
- `$width` (`int`)
- `$height` (`int`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecreatetruecolor` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecreatetruecolor.md).
