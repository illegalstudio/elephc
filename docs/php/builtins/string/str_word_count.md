---
title: "str_word_count()"
description: "Counts the words in a string, or returns them as a list or byte-offset map."
sidebar:
  order: 525
---

## str_word_count()

```php
function str_word_count(string $string, int $format = 0, string $characters = null): array|int
```

Counts the words in a string, or returns them as a list or byte-offset map.

**Parameters**:
- `$string` (`string`)
- `$format` (`int`), default `0`, optional
- `$characters` (`string`), default `null`, optional

**Returns**: `array|int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/str_word_count.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/str_word_count.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `str_word_count` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/str_word_count.md).
