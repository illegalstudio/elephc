---
title: "imagesetthickness()"
description: "Sets the line thickness used by subsequent drawing."
sidebar:
  order: 532
---

## imagesetthickness()

```php
function imagesetthickness(mixed $image, int $thickness): bool
```

Sets the line thickness used by subsequent drawing.

**Parameters**:
- `$image` (`mixed`)
- `$thickness` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagesetthickness` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagesetthickness.md).
