---
title: "gmp_strval()"
description: "Formats a GMP number in a base."
sidebar:
  order: 636
---

## gmp_strval()

```php
function gmp_strval(object|string|int $num, int $base = 10): string
```

Formats a GMP number in a base.

**Parameters**:
- `$num` (`object|string|int`)
- `$base` (`int`), default `10`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_strval.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_strval.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_strval` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_strval.md).
