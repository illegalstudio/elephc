---
title: "srand()"
description: "Seed the random number generator."
sidebar:
  order: 604
---

## srand()

```php
function srand(?int $seed = null, int $mode = MT_RAND_MT19937): void
```

Seed the random number generator.

**Parameters**:
- `$seed` (`?int`), default `null`, optional
- `$mode` (`int`), default `MT_RAND_MT19937`, optional

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/srand.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/srand.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `srand` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/srand.md).
