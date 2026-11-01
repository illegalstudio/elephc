---
title: "var_export()"
description: "Renders a parsable string representation of a variable."
sidebar:
  order: 419
---

## var_export()

```php
function var_export(mixed $value, bool $return = false): mixed
```

Renders a parsable string representation of a variable.

**Parameters**:
- `$value` (`mixed`)
- `$return` (`bool`), default `false`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected var_export prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/var_export.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/var_export.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `var_export` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/var_export.md).
