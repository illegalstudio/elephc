---
title: "filter_var()"
description: "Filters a variable with a specified filter."
sidebar:
  order: 694
---

## filter_var()

```php
function filter_var(mixed $value, int $filter = 516, mixed $options = 0): mixed
```

Filters a variable with a specified filter.

**Parameters**:
- `$value` (`mixed`)
- `$filter` (`int`), default `516`, optional
- `$options` (`mixed`), default `0`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/types/filter_var.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/filter_var.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `filter_var` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/filter_var.md).
