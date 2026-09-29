---
title: "gd_info()"
description: "Returns the GD version and which image formats this build supports."
sidebar:
  order: 453
---

## gd_info()

```php
function gd_info(): array
```

Returns the GD version and which image formats this build supports.

**Parameters**: none.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gd_info` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/gd_info.md).
