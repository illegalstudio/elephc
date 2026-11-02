---
title: "gmp_prob_prime()"
description: "Probable-prime test."
sidebar:
  order: 624
---

## gmp_prob_prime()

```php
function gmp_prob_prime(object|string|int $num, int $repetitions = 10): int
```

Probable-prime test.

**Parameters**:
- `$num` (`object|string|int`)
- `$repetitions` (`int`), default `10`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_prob_prime.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_prob_prime.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_prob_prime` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_prob_prime.md).
