---
title: "ucwords()"
description: "Uppercases the first character of each word in a string."
sidebar:
  order: 409
---

## ucwords()

```php
function ucwords(string $string, string $separators = ' \t\r\n\x0c\x0b'): string
```

Uppercases the first character of each word in a string.

**Parameters**:
- `$string` (`string`)
- `$separators` (`string`), default `' \t\r\n\x0c\x0b'`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/ucwords.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/ucwords.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ucwords` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/ucwords.md).

