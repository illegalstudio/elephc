---
title: "date_default_timezone_get()"
description: "Gets the default timezone."
sidebar:
  order: 93
---

## date_default_timezone_get()

```php
function date_default_timezone_get(): string
```

Gets the default timezone.

**Parameters**: none.

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/time/date_default_timezone_get.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/time/date_default_timezone_get.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `date_default_timezone_get` is implemented in the compiler, see [the internals page](../../../internals/builtins/date/date_default_timezone_get.md).
