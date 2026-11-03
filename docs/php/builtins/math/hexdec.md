---
title: "hexdec()"
description: "Converts a hexadecimal string to its decimal number."
sidebar:
  order: 655
---

## hexdec()

```php
function hexdec(string $hex_string): mixed
```

Converts a hexadecimal string to its decimal number.

**Parameters**:
- `$hex_string` (`string`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/math/hexdec.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/hexdec.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `hexdec` is implemented in the compiler, see [the internals page](../../../internals/builtins/math/hexdec.md).
