---
title: "trim()"
description: "Strips whitespace (or other characters) from the beginning and end of a string."
sidebar:
  order: 407
---

## trim()

```php
function trim(string $string, string $characters = ' \n\r\t\x0b\x0c\x00'): string
```

Strips whitespace (or other characters) from the beginning and end of a string.

**Parameters**:
- `$string` (`string`)
- `$characters` (`string`), default `' \n\r\t\x0b\x0c\x00'`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/trim.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/trim.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `trim` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/trim.md).

