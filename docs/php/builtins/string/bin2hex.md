---
title: "bin2hex()"
description: "Converts binary data into its hexadecimal string representation."
sidebar:
  order: 366
---

## bin2hex()

```php
function bin2hex(string $string): string
```

Converts binary data into its hexadecimal string representation.

**Parameters**:
- `$string` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/bin2hex.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/bin2hex.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `bin2hex` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/bin2hex.md).
