---
title: "vsprintf()"
description: "Returns a formatted string using an array of values."
sidebar:
  order: 426
---

## vsprintf()

```php
function vsprintf(string $format, array $values): string
```

Returns a formatted string using an array of values.

**Parameters**:
- `$format` (`string`)
- `$values` (`array`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/formatting/vsprintf.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/formatting/vsprintf.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `vsprintf` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/vsprintf.md).

