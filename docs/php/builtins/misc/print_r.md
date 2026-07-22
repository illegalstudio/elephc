---
title: "print_r()"
description: "Prints human-readable information about a variable."
sidebar:
  order: 299
---

## print_r()

```php
function print_r(mixed $value, bool $return = false): mixed
```

Prints human-readable information about a variable.

**Parameters**:
- `$value` (`mixed`)
- `$return` (`bool`), default `false`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/print_r.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/print_r.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `print_r` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/print_r.md).
