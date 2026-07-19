---
title: "min()"
description: "Find lowest value."
sidebar:
  order: 275
---

## min()

```php
function min(mixed $value, ...$values): mixed
```

Find lowest value.

**Parameters**:
- `$value` (`mixed`)
- `...$values` — variadic: collects excess arguments into `$values`.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/min.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/min.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `min` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/min.md).

