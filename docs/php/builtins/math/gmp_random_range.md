---
title: "gmp_random_range()"
description: "Generates a random GMP number in a range."
sidebar:
  order: 626
---

## gmp_random_range()

```php
function gmp_random_range(object|string|int $min, object|string|int $max): object
```

Generates a random GMP number in a range.

**Parameters**:
- `$min` (`object|string|int`)
- `$max` (`object|string|int`)

**Returns**: `object`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_random_range.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_random_range.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_random_range` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_random_range.md).
