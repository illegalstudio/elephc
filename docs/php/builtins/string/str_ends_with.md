---
title: "str_ends_with()"
description: "Checks if a string ends with a given substring."
sidebar:
  order: 403
---

## str_ends_with()

```php
function str_ends_with(string $haystack, string $needle): bool
```

Checks if a string ends with a given substring.

**Parameters**:
- `$haystack` (`string`)
- `$needle` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/str_ends_with.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/str_ends_with.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `str_ends_with` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/str_ends_with.md).
