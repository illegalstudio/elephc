---
title: "imagesavealpha()"
description: "Controls whether full alpha channel information is saved with the image."
sidebar:
  order: 528
---

## imagesavealpha()

```php
function imagesavealpha(mixed $image, bool $enable): bool
```

Controls whether full alpha channel information is saved with the image.

**Parameters**:
- `$image` (`mixed`)
- `$enable` (`bool`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagesavealpha` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagesavealpha.md).
