---
title: "imagecolorclosestalpha()"
description: "Returns the palette index closest to the requested color with alpha."
sidebar:
  order: 472
---

## imagecolorclosestalpha()

```php
function imagecolorclosestalpha(mixed $image, int $red, int $green, int $blue, int $alpha): int
```

Returns the palette index closest to the requested color with alpha.

**Parameters**:
- `$image` (`mixed`)
- `$red` (`int`)
- `$green` (`int`)
- `$blue` (`int`)
- `$alpha` (`int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecolorclosestalpha` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecolorclosestalpha.md).
