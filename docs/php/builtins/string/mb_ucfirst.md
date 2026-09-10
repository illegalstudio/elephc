---
title: "mb_ucfirst()"
description: "Converts the first character to Unicode title case."
sidebar:
  order: 859
---

## mb_ucfirst()

```php
function mb_ucfirst(string $string, ?string $encoding = null): string
```

Converts the first character to Unicode title case.

**Parameters**:
- `$string` (`string`)
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_ucfirst.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_ucfirst.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_ucfirst` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_ucfirst.md).
