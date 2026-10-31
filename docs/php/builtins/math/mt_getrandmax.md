---
title: "mt_getrandmax()"
description: "Show largest possible random value."
sidebar:
  order: 652
---

## mt_getrandmax()

```php
function mt_getrandmax(): int
```

Show largest possible random value.

**Parameters**: none.

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected random prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mt_getrandmax` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/mt_getrandmax.md).
