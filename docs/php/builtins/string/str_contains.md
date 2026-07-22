---
title: "str_contains()"
description: "Determines if a string contains a given substring."
sidebar:
  order: 402
---

## str_contains()

```php
function str_contains(string $haystack, string $needle): bool
```

Determines if a string contains a given substring.

**Parameters**:
- `$haystack` (`string`)
- `$needle` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/str_contains.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/str_contains.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `str_contains` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/str_contains.md).
