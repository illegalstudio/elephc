---
title: "str_ireplace()"
description: "Case-insensitive version of str_replace()."
sidebar:
  order: 516
---

## str_ireplace()

```php
function str_ireplace(string $search, string $replace, string $subject, int $count = null): string
```

Case-insensitive version of str_replace().

**Parameters**:
- `$search` (`string`)
- `$replace` (`string`)
- `$subject` (`string`)
- `$count` (`int`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/str_ireplace.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/str_ireplace.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `str_ireplace` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/str_ireplace.md).
