---
title: "max()"
description: "Find highest value."
sidebar:
  order: 274
---

## max()

```php
function max(mixed $value, ...$values): mixed
```

Find highest value.

**Parameters**:
- `$value` (`mixed`)
- `...$values` — variadic: collects excess arguments into `$values`.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/max.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/max.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `max` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/max.md).

