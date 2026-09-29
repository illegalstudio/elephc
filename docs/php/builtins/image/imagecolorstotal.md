---
title: "imagecolorstotal()"
description: "Returns how many colors a palette image holds."
sidebar:
  order: 480
---

## imagecolorstotal()

```php
function imagecolorstotal(mixed $image): int
```

Returns how many colors a palette image holds.

**Parameters**:
- `$image` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `imagecolorstotal` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/imagecolorstotal.md).
