---
title: "bindec()"
description: "Converts a binary string to its decimal number."
sidebar:
  order: 592
---

## bindec()

```php
function bindec(string $binary_string): mixed
```

Converts a binary string to its decimal number.

**Parameters**:
- `$binary_string` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/bindec.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/bindec.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `bindec` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/bindec.md).
