---
title: "vprintf()"
description: "Outputs a formatted string using an array of values."
sidebar:
  order: 427
---

## vprintf()

```php
function vprintf(string $format, array $values): int
```

Outputs a formatted string using an array of values.

**Parameters**:
- `$format` (`string`)
- `$values` (`array`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/formatting/vprintf.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/formatting/vprintf.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `vprintf` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/vprintf.md).
