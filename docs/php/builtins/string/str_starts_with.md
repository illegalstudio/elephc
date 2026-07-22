---
title: "str_starts_with()"
description: "Checks if a string starts with a given substring."
sidebar:
  order: 409
---

## str_starts_with()

```php
function str_starts_with(string $haystack, string $needle): bool
```

Checks if a string starts with a given substring.

**Parameters**:
- `$haystack` (`string`)
- `$needle` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/str_starts_with.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/str_starts_with.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `str_starts_with` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/str_starts_with.md).
