---
title: "intdiv()"
description: "Integer division."
sidebar:
  order: 269
---

## intdiv()

```php
function intdiv(int $num1, int $num2): int
```

Integer division.

**Parameters**:
- `$num1` (`int`)
- `$num2` (`int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/intdiv.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/intdiv.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `intdiv` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/intdiv.md).
