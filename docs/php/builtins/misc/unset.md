---
title: "unset()"
description: "Unsets the given variables."
sidebar:
  order: 303
---

## unset()

```php
function unset(mixed $var, ...$vars): void
```

Unsets the given variables.

**Parameters**:
- `$var` (`mixed`)
- `...$vars` — variadic: collects excess arguments into `$vars`.

**Returns**: `void`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/unset.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/unset.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `unset` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/unset.md).
