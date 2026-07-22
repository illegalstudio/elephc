---
title: "date()"
description: "Formats a local time/date."
sidebar:
  order: 92
---

## date()

```php
function date(string $format, int $timestamp = null): string
```

Formats a local time/date.

**Parameters**:
- `$format` (`string`)
- `$timestamp` (`int`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/time/date.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/date.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `date` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date.md).
