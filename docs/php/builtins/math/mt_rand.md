---
title: "mt_rand()"
description: "Generate a random value via the Mersenne Twister Random Number Generator."
sidebar:
  order: 278
---

## mt_rand()

```php
function mt_rand(int $min, int $max): int
```

Generate a random value via the Mersenne Twister Random Number Generator.

**Parameters**:
- `$min` (`int`)
- `$max` (`int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/mt_rand.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/mt_rand.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `mt_rand` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/mt_rand.md).
