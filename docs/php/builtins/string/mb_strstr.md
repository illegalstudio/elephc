---
title: "mb_strstr()"
description: "Returns text before or from the first matching substring, or false."
sidebar:
  order: 851
---

## mb_strstr()

```php
function mb_strstr(string $haystack, string $needle, bool $before_needle = false, ?string $encoding = null): string|false
```

Returns text before or from the first matching substring, or false.

**Parameters**:
- `$haystack` (`string`)
- `$needle` (`string`)
- `$before_needle` (`bool`), default `false`, optional
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string|false`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_strstr.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_strstr.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_strstr` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_strstr.md).
