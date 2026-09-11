---
title: "mb_detect_encoding()"
description: "Guesses the most likely candidate encoding using the request's detection settings."
sidebar:
  order: 805
---

## mb_detect_encoding()

```php
function mb_detect_encoding(string $string, array|string|null $encodings = null, bool $strict = false): string|false
```

Guesses the most likely candidate encoding using the request's detection settings.

**Parameters**:
- `$string` (`string`)
- `$encodings` (`array|string|null`), default `null`, optional
- `$strict` (`bool`), default `false`, optional

**Returns**: `string|false`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_detect_encoding.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_detect_encoding.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_detect_encoding` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_detect_encoding.md).
