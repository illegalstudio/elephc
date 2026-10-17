---
title: "mt_srand()"
description: "Seeds the Mersenne Twister Random Number Generator."
sidebar:
  order: 593
---

## mt_srand()

```php
function mt_srand(?int $seed = null, int $mode = MT_RAND_MT19937): void
```

Seeds the Mersenne Twister Random Number Generator.

**Parameters**:
- `$seed` (`?int`), default `null`, optional
- `$mode` (`int`), default `MT_RAND_MT19937`, optional

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/mt_srand.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/mt_srand.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mt_srand` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/mt_srand.md).
