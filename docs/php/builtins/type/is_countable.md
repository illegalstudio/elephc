---
title: "is_countable()"
description: "Verifies that the contents of a variable is a countable value."
sidebar:
  order: 960
---

## is_countable()

```php
function is_countable(mixed $value): bool
```

Verifies that the contents of a variable is a countable value.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/types/is_countable.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/is_countable.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `is_countable` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/is_countable.md).
