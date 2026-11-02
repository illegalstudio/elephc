---
title: "gmp_binomial()"
description: "Computes a binomial coefficient."
sidebar:
  order: 592
---

## gmp_binomial()

```php
function gmp_binomial(object|string|int $num, int $k): object
```

Computes a binomial coefficient.

**Parameters**:
- `$num` (`object|string|int`)
- `$k` (`int`)

**Returns**: `object`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_binomial.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_binomial.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_binomial` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_binomial.md).
