---
title: "mb_strlen()"
description: "Returns the character count of a string in the requested encoding."
sidebar:
  order: 375
---

## mb_strlen()

```php
function mb_strlen(string $string, string $encoding = null): int
```

Returns the character count of a string in the requested encoding.

**Parameters**:
- `$string` (`string`)
- `$encoding` (`string`), default `null`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_strlen.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_strlen.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `mb_strlen` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_strlen.md).

