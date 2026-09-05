---
title: "imagecrop()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 494
---

## imagecrop()

```php
function imagecrop(mixed $image, mixed $rect = ['x' => 0, 'y' => 0, 'width' => 0, 'height' => 0]): mixed
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image` (`mixed`)
- `$rect` (`mixed`), default `['x' => 0, 'y' => 0, 'width' => 0, 'height' => 0]`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecrop` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecrop.md).
