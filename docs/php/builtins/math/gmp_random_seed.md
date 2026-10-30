---
title: "gmp_random_seed()"
description: "Seeds PHP's GMP random generator."
sidebar:
  order: 627
---

## gmp_random_seed()

```php
function gmp_random_seed(object|string|int $seed): void
```

Seeds PHP's GMP random generator.

**Parameters**:
- `$seed` (`object|string|int`)

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_random_seed.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_random_seed.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_random_seed` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_random_seed.md).
