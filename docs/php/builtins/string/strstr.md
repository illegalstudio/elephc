---
title: "strstr()"
description: "Returns the portion of a string starting at the first occurrence of a substring, or false."
sidebar:
  order: 473
---

## strstr()

```php
function strstr(string $haystack, string $needle, bool $before_needle = false): mixed
```

Returns the portion of a string starting at the first occurrence of a substring, or false.

**Parameters**:
- `$haystack` (`string`)
- `$needle` (`string`)
- `$before_needle` (`bool`), default `false`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/strstr.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/strstr.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `strstr` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strstr.md).
