---
title: "imageantialias()"
description: "Turns antialiased drawing on or off for lines and polygons."
sidebar:
  order: 461
---

## imageantialias()

```php
function imageantialias(mixed $image, bool $enable): bool
```

Turns antialiased drawing on or off for lines and polygons.

**Parameters**:
- `$image` (`mixed`)
- `$enable` (`bool`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imageantialias` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imageantialias.md).
