---
title: "imagecolorresolve()"
description: "Returns the palette index of a color, allocating or approximating it."
sidebar:
  order: 478
---

## imagecolorresolve()

```php
function imagecolorresolve(mixed $image, int $red, int $green, int $blue): int
```

Returns the palette index of a color, allocating or approximating it.

**Parameters**:
- `$image` (`mixed`)
- `$red` (`int`)
- `$green` (`int`)
- `$blue` (`int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecolorresolve` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecolorresolve.md).
