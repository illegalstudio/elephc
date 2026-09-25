---
title: "mb_convert_variables()"
description: "Detects one source encoding and converts strings in variables, nested arrays, and object properties by reference."
sidebar:
  order: 833
---

## mb_convert_variables()

```php
function mb_convert_variables(string $to_encoding, array|string $from_encoding, mixed $var, ...$vars): string|false
```

Detects one source encoding and converts strings in variables, nested arrays, and object properties by reference.

**Parameters**:
- `$to_encoding` (`string`)
- `$from_encoding` (`array|string`)
- `$var` (`mixed`), passed by reference
- `...$vars` - variadic: collects excess arguments into `$vars`.

**Returns**: `string|false`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_convert_variables.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_convert_variables.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_convert_variables` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_convert_variables.md).
