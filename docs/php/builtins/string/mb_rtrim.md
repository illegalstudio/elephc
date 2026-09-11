---
title: "mb_rtrim()"
description: "Removes Unicode whitespace or the specified characters from the end."
sidebar:
  order: 836
---

## mb_rtrim()

```php
function mb_rtrim(string $string, ?string $characters = null, ?string $encoding = null): string
```

Removes Unicode whitespace or the specified characters from the end.

**Parameters**:
- `$string` (`string`)
- `$characters` (`?string`), default `null`, optional
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_rtrim.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_rtrim.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_rtrim` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_rtrim.md).
