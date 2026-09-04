---
title: "imagecolorclosestalpha()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 467
---

## imagecolorclosestalpha()

```php
function imagecolorclosestalpha(mixed $image, int $red, int $green, int $blue, int $alpha): int
```

Implemented by the compiler-injected image prelude.

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

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecolorclosestalpha` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecolorclosestalpha.md).
