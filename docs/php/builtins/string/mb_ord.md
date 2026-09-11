---
title: "mb_ord()"
description: "Returns the first Unicode codepoint, or false for malformed input."
sidebar:
  order: 830
---

## mb_ord()

```php
function mb_ord(string $string, ?string $encoding = null): int|false
```

Returns the first Unicode codepoint, or false for malformed input.

**Parameters**:
- `$string` (`string`)
- `$encoding` (`?string`), default `null`, optional

**Returns**: `int|false`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_ord.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_ord.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_ord` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_ord.md).
