---
title: "gmp_rootrem()"
description: "Integer nth root and remainder."
sidebar:
  order: 629
---

## gmp_rootrem()

```php
function gmp_rootrem(object|string|int $num, int $nth): array
```

Integer nth root and remainder.

**Parameters**:
- `$num` (`object|string|int`)
- `$nth` (`int`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_rootrem.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_rootrem.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_rootrem` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_rootrem.md).
