---
title: "var_dump()"
description: "Dumps information about a variable, including its type and value."
sidebar:
  order: 304
---

## var_dump()

```php
function var_dump(mixed $value, ...$values): void
```

Dumps information about a variable, including its type and value.

**Parameters**:
- `$value` (`mixed`)
- `...$values` — variadic: collects excess arguments into `$values`.

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/var_dump.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/var_dump.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `var_dump` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/var_dump.md).
