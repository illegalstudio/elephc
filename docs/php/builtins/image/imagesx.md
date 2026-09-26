---
title: "imagesx()"
description: "Returns an image's width in pixels."
sidebar:
  order: 535
---

## imagesx()

```php
function imagesx(mixed $image): int
```

Returns an image's width in pixels.

**Parameters**:
- `$image` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagesx` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagesx.md).
