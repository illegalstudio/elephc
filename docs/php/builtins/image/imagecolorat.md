---
title: "imagecolorat()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 465
---

## imagecolorat()

```php
function imagecolorat(mixed $image, int $x, int $y): int
```

Implemented by the compiler-injected image prelude.

**Parameters**:
- `$image` (`mixed`)
- `$x` (`int`)
- `$y` (`int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecolorat` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecolorat.md).
