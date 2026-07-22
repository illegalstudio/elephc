---
title: "printf()"
description: "Outputs a formatted string."
sidebar:
  order: 395
---

## printf()

```php
function printf(string $format, ...$values): int
```

Outputs a formatted string.

**Parameters**:
- `$format` (`string`)
- `...$values` — variadic: collects excess arguments into `$values`.

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/formatting/printf.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/formatting/printf.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `printf` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/printf.md).
