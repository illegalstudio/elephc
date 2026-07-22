---
title: "log()"
description: "Natural logarithm."
sidebar:
  order: 273
---

## log()

```php
function log(float $num, float $base = 2.718281828459045): float
```

Natural logarithm.

**Parameters**:
- `$num` (`float`)
- `$base` (`float`), default `2.718281828459045`, optional

**Returns**: `float`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/log.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/log.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `log` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/log.md).
