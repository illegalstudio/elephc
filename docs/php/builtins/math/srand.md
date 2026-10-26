---
title: "srand()"
description: "Seed the random number generator."
sidebar:
  order: 607
---

## srand()

```php
function srand(?int $seed = null, int $mode = 0): void
```

Seed the random number generator.

**Parameters**:
- `$seed` (`?int`), default `null`, optional
- `$mode` (`int`), default `0`, optional

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected random prelude.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `srand` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/srand.md).
