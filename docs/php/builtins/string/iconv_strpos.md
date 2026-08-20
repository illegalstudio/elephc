---
title: "iconv_strpos()"
description: "Finds the first character position of a needle in a string."
sidebar:
  order: 423
---

## iconv_strpos()

```php
function iconv_strpos(string $haystack, string $needle, int $offset = 0, string $encoding = null): mixed
```

Finds the first character position of a needle in a string.

**Parameters**:
- `$haystack` (`string`)
- `$needle` (`string`)
- `$offset` (`int`), default `0`, optional
- `$encoding` (`string`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/iconv_strpos.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/iconv_strpos.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `iconv_strpos` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/iconv_strpos.md).
