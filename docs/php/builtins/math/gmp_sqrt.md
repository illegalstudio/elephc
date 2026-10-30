---
title: "gmp_sqrt()"
description: "Integer square root."
sidebar:
  order: 634
---

## gmp_sqrt()

```php
function gmp_sqrt(object|string|int $num): object
```

Integer square root.

**Parameters**:
- `$num` (`object|string|int`)

**Returns**: `object`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_sqrt.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_sqrt.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_sqrt` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_sqrt.md).
