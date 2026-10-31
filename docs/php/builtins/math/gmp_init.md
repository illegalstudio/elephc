---
title: "gmp_init()"
description: "Creates a GMP number from an integer or string."
sidebar:
  order: 607
---

## gmp_init()

```php
function gmp_init(string|int $num, int $base = 0): object
```

Creates a GMP number from an integer or string.

**Parameters**:
- `$num` (`string|int`)
- `$base` (`int`), default `0`, optional

**Returns**: `object`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/gmp/gmp_init.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/gmp/gmp_init.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `gmp_init` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/gmp_init.md).
