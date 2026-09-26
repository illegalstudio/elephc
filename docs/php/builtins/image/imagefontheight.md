---
title: "imagefontheight()"
description: "Returns the pixel height of a built-in font."
sidebar:
  order: 510
---

## imagefontheight()

```php
function imagefontheight(int $font): int
```

Returns the pixel height of a built-in font.

**Parameters**:
- `$font` (`int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagefontheight` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagefontheight.md).
