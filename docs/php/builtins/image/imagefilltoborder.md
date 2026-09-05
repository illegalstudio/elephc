---
title: "imagefilltoborder()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 504
---

## imagefilltoborder()

```php
function imagefilltoborder(mixed $image, int $x, int $y, int $border_color, int $color): bool
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image` (`mixed`)
- `$x` (`int`)
- `$y` (`int`)
- `$border_color` (`int`)
- `$color` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagefilltoborder` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagefilltoborder.md).
