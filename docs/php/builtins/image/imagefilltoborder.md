---
title: "imagefilltoborder()"
description: "Flood-fills from a point until it reaches a border color."
sidebar:
  order: 507
---

## imagefilltoborder()

```php
function imagefilltoborder(mixed $image, int $x, int $y, int $border_color, int $color): bool
```

Flood-fills from a point until it reaches a border color.

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

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagefilltoborder` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagefilltoborder.md).
