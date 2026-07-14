---
title: "rtrim()"
description: "Strips whitespace (or other characters) from the end of a string."
sidebar:
  order: 383
---

## rtrim()

```php
function rtrim(string $string, string $characters = ' \n\r\t\x0b\x0c\x00'): string
```

Strips whitespace (or other characters) from the end of a string.

**Parameters**:
- `$string` (`string`)
- `$characters` (`string`), default `' \n\r\t\x0b\x0c\x00'`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/rtrim.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/rtrim.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `rtrim` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/rtrim.md).

