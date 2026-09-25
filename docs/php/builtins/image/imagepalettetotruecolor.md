---
title: "imagepalettetotruecolor()"
description: "Converts a palette image to truecolor in place."
sidebar:
  order: 526
---

## imagepalettetotruecolor()

```php
function imagepalettetotruecolor(mixed $image): bool
```

Converts a palette image to truecolor in place.

**Parameters**:
- `$image` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagepalettetotruecolor` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagepalettetotruecolor.md).
