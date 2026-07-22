---
title: "strtoupper()"
description: "Converts a string to uppercase."
sidebar:
  order: 419
---

## strtoupper()

```php
function strtoupper(string $string): string
```

Converts a string to uppercase.

**Parameters**:
- `$string` (`string`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/strtoupper.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/strtoupper.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `strtoupper` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strtoupper.md).
