---
title: "gmp_root()"
description: "Integer nth root."
sidebar:
  order: 628
---

## gmp_root()

```php
function gmp_root(object|string|int $num, int $nth): object
```

Integer nth root.

**Parameters**:
- `$num` (`object|string|int`)
- `$nth` (`int`)

**Returns**: `object`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_root.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_root.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_root` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_root.md).
