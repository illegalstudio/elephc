---
title: "str_repeat()"
description: "Repeats a string a given number of times."
sidebar:
  order: 390
---

## str_repeat()

```php
function str_repeat(string $string, int $times): string
```

Repeats a string a given number of times.

**Parameters**:
- `$string` (`string`)
- `$times` (`int`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/str_repeat.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/str_repeat.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `str_repeat` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/str_repeat.md).

