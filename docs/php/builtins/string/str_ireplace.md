---
title: "str_ireplace()"
description: "Case-insensitive version of str_replace(): replaces every occurrence of each search string, in a string or in every element of an array."
sidebar:
  order: 913
---

## str_ireplace()

```php
function str_ireplace(array|string $search, array|string $replace, array|string $subject, int $count = null): array|string
```

Case-insensitive version of str_replace(): replaces every occurrence of each search string, in a string or in every element of an array.

**Parameters**:
- `$search` (`array|string`)
- `$replace` (`array|string`)
- `$subject` (`array|string`)
- `$count` (`int`), passed by reference, default `null`, optional

**Returns**: `array|string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/str_ireplace.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/str_ireplace.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `str_ireplace` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/str_ireplace.md).
