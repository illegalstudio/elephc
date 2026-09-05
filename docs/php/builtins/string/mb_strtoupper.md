---
title: "mb_strtoupper()"
description: "Converts a string to uppercase using Unicode full case mapping."
sidebar:
  order: 469
---

## mb_strtoupper()

```php
function mb_strtoupper(string $string, string $encoding = null): string
```

Converts a string to uppercase using Unicode full case mapping.

**Parameters**:
- `$string` (`string`)
- `$encoding` (`string`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_strtoupper.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_strtoupper.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_strtoupper` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_strtoupper.md).
