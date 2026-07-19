---
title: "abs()"
description: "Absolute value."
sidebar:
  order: 252
---

## abs()

```php
function abs(int $num): mixed
```

Absolute value.

**Parameters**:
- `$num` (`int`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/abs.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/abs.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `abs` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/abs.md).

