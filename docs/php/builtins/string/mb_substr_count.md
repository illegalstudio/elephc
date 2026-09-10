---
title: "mb_substr_count()"
description: "Counts non-overlapping occurrences of an encoded substring."
sidebar:
  order: 857
---

## mb_substr_count()

```php
function mb_substr_count(string $haystack, string $needle, ?string $encoding = null): int
```

Counts non-overlapping occurrences of an encoded substring.

**Parameters**:
- `$haystack` (`string`)
- `$needle` (`string`)
- `$encoding` (`?string`), default `null`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_substr_count.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_substr_count.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_substr_count` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_substr_count.md).
