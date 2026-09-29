---
title: "imagesy()"
description: "Returns an image's height in pixels."
sidebar:
  order: 536
---

## imagesy()

```php
function imagesy(mixed $image): int
```

Returns an image's height in pixels.

**Parameters**:
- `$image` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagesy` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagesy.md).
