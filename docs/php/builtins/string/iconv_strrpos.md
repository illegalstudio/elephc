---
title: "iconv_strrpos()"
description: "Finds the last character position of a needle in a string."
sidebar:
  order: 486
---

## iconv_strrpos()

```php
function iconv_strrpos(string $haystack, string $needle, string $encoding = null): mixed
```

Finds the last character position of a needle in a string.

**Parameters**:
- `$haystack` (`string`)
- `$needle` (`string`)
- `$encoding` (`string`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/iconv_strrpos.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/iconv_strrpos.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `iconv_strrpos` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/iconv_strrpos.md).
