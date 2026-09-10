---
title: "mb_convert_encoding()"
description: "Converts strings and recursive array keys and values between character encodings."
sidebar:
  order: 801
---

## mb_convert_encoding()

```php
function mb_convert_encoding(array|string $string, string $to_encoding, array|string|null $from_encoding = null): array|string|false
```

Converts strings and recursive array keys and values between character encodings.

**Parameters**:
- `$string` (`array|string`)
- `$to_encoding` (`string`)
- `$from_encoding` (`array|string|null`), default `null`, optional

**Returns**: `array|string|false`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_convert_encoding.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_convert_encoding.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_convert_encoding` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_convert_encoding.md).
