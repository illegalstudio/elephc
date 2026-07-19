---
title: "htmlspecialchars()"
description: "Converts the HTML special characters in a string into their entities."
sidebar:
  order: 367
---

## htmlspecialchars()

```php
function htmlspecialchars(string $string, int $flags = 11, string $encoding = 'UTF-8'): string
```

Converts the HTML special characters in a string into their entities.

**Parameters**:
- `$string` (`string`)
- `$flags` (`int`), default `11`, optional
- `$encoding` (`string`), default `'UTF-8'`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/htmlspecialchars.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/htmlspecialchars.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `htmlspecialchars` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/htmlspecialchars.md).

