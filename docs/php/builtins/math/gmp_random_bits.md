---
title: "gmp_random_bits()"
description: "Generates a random GMP number with the given bit count."
sidebar:
  order: 625
---

## gmp_random_bits()

```php
function gmp_random_bits(int $bits): object
```

Generates a random GMP number with the given bit count.

**Parameters**:
- `$bits` (`int`)

**Returns**: `object`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_random_bits.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_random_bits.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_random_bits` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_random_bits.md).
