---
title: "mb_convert_kana()"
description: "Converts Japanese width and kana according to the requested mode flags."
sidebar:
  order: 802
---

## mb_convert_kana()

```php
function mb_convert_kana(string $string, string $mode = 'KV', ?string $encoding = null): string
```

Converts Japanese width and kana according to the requested mode flags.

**Parameters**:
- `$string` (`string`)
- `$mode` (`string`), default `'KV'`, optional
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_convert_kana.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_convert_kana.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_convert_kana` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_convert_kana.md).
