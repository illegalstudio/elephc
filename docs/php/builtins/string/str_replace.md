---
title: "str_replace()"
description: "Replaces all occurrences of a search string with a replacement string."
sidebar:
  order: 414
---

## str_replace()

```php
function str_replace(string $search, string $replace, string $subject, int $count = null): string
```

Replaces all occurrences of a search string with a replacement string.

**Parameters**:
- `$search` (`string`)
- `$replace` (`string`)
- `$subject` (`string`)
- `$count` (`int`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/str_replace.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/str_replace.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `str_replace` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/str_replace.md).
