---
title: "mb_lcfirst()"
description: "Converts the first character to Unicode lowercase."
sidebar:
  order: 827
---

## mb_lcfirst()

```php
function mb_lcfirst(string $string, ?string $encoding = null): string
```

Converts the first character to Unicode lowercase.

**Parameters**:
- `$string` (`string`)
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_lcfirst.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_lcfirst.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_lcfirst` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_lcfirst.md).
