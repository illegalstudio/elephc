---
title: "gmdate()"
description: "Formats a GMT/UTC date and time."
sidebar:
  order: 94
---

## gmdate()

```php
function gmdate(string $format, int $timestamp = null): string
```

Formats a GMT/UTC date and time.

**Parameters**:
- `$format` (`string`)
- `$timestamp` (`int`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/time/gmdate.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/gmdate.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `gmdate` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/gmdate.md).

