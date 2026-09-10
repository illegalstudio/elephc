---
title: "mb_ltrim()"
description: "Removes Unicode whitespace or the specified characters from the beginning."
sidebar:
  order: 829
---

## mb_ltrim()

```php
function mb_ltrim(string $string, ?string $characters = null, ?string $encoding = null): string
```

Removes Unicode whitespace or the specified characters from the beginning.

**Parameters**:
- `$string` (`string`)
- `$characters` (`?string`), default `null`, optional
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_ltrim.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_ltrim.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_ltrim` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_ltrim.md).
