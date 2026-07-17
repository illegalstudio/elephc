---
title: "getdate()"
description: "Returns date/time information."
sidebar:
  order: 93
---

## getdate()

```php
function getdate(int $timestamp = null): array
```

Returns date/time information.

**Parameters**:
- `$timestamp` (`int`), default `null`, optional

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/time/getdate.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/getdate.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `getdate` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/getdate.md).

