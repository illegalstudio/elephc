---
title: "gd_info()"
description: "Implemented by the compiler-injected image prelude."
sidebar:
  order: 450
---

## gd_info()

```php
function gd_info(): mixed
```

Implemented by the compiler-injected image prelude.

**Parameters**: none.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected image prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gd_info` is implemented in the compiler, see [the internals page](../../../internals/builtins/image/gd_info.md).
