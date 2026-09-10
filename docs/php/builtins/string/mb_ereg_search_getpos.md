---
title: "mb_ereg_search_getpos()"
description: "Reads the current byte position in the retained multibyte regex subject."
sidebar:
  order: 814
---

## mb_ereg_search_getpos()

```php
function mb_ereg_search_getpos(): int
```

Reads the current byte position in the retained multibyte regex subject.

**Parameters**: none.

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_ereg_search_getpos.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_ereg_search_getpos.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_ereg_search_getpos` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_ereg_search_getpos.md).
