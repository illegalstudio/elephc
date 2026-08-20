---
title: "iconv_substr()"
description: "Extracts a character-indexed slice of a string."
sidebar:
  order: 425
---

## iconv_substr()

```php
function iconv_substr(string $string, int $offset, int $length = null, string $encoding = null): mixed
```

Extracts a character-indexed slice of a string.

**Parameters**:
- `$string` (`string`)
- `$offset` (`int`)
- `$length` (`int`), default `null`, optional
- `$encoding` (`string`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/iconv_substr.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/iconv_substr.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `iconv_substr` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/iconv_substr.md).
