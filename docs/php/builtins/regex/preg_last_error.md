---
title: "preg_last_error()"
description: "Returns the `PREG_*_ERROR` code of the last PCRE function call: `PREG_INTERNAL_ERROR` for an invalid pattern or offset, `PREG_BACKTRACK_LIMIT_ERROR` when PCRE2 runs out of match or heap budget, `PREG_BAD_UTF8_ERROR` for a subject PCRE2 rejects as invalid UTF-8 under `/u`."
---

## preg_last_error()

```php
function preg_last_error(): int
```

Returns the `PREG_*_ERROR` code of the last PCRE function call: `PREG_INTERNAL_ERROR` for an invalid pattern or offset, `PREG_BACKTRACK_LIMIT_ERROR` when PCRE2 runs out of match or heap budget, `PREG_BAD_UTF8_ERROR` for a subject PCRE2 rejects as invalid UTF-8 under `/u`.

**Parameters**: none.

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `preg_last_error` is implemented in the compiler, see [the internals page](../../../internals/builtins/regex/preg_last_error.md).
