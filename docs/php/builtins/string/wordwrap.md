---
title: "wordwrap()"
description: "Wraps a string to a given number of characters."
sidebar:
  order: 436
---

## wordwrap()

```php
function wordwrap(string $string, int $width = 75, string $break = '\n', bool $cut_long_words = false): string
```

Wraps a string to a given number of characters.

**Parameters**:
- `$string` (`string`)
- `$width` (`int`), default `75`, optional
- `$break` (`string`), default `'\n'`, optional
- `$cut_long_words` (`bool`), default `false`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/wordwrap.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/wordwrap.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `wordwrap` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/wordwrap.md).
