---
title: "is_bool()"
description: "Checks whether a variable is a boolean."
sidebar:
  order: 560
---

## is_bool()

```php
function is_bool(mixed $value): bool
```

Checks whether a variable is a boolean.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/types/is_bool.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/is_bool.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `is_bool` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/is_bool.md).
