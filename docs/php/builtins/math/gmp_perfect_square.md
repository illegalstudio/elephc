---
title: "gmp_perfect_square()"
description: "Checks whether a GMP number is a perfect square."
sidebar:
  order: 620
---

## gmp_perfect_square()

```php
function gmp_perfect_square(object|string|int $num): bool
```

Checks whether a GMP number is a perfect square.

**Parameters**:
- `$num` (`object|string|int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_perfect_square.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_perfect_square.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_perfect_square` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_perfect_square.md).
