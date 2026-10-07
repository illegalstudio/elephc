---
title: "str_replace()"
description: "Replaces all occurrences of each search string with its replacement, in a string or in every element of an array."
sidebar:
  order: 916
---

## str_replace()

```php
function str_replace(array|string $search, array|string $replace, array|string $subject, int $count = null): array|string
```

Replaces all occurrences of each search string with its replacement, in a string or in every element of an array.

**Parameters**:
- `$search` (`array|string`)
- `$replace` (`array|string`)
- `$subject` (`array|string`)
- `$count` (`int`), passed by reference, default `null`, optional

**Returns**: `array|string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/str_replace.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/str_replace.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `str_replace` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/str_replace.md).
