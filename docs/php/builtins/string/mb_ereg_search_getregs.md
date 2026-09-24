---
title: "mb_ereg_search_getregs()"
description: "Returns the retained multibyte regex captures from the last successful progressive search."
sidebar:
  order: 845
---

## mb_ereg_search_getregs()

```php
function mb_ereg_search_getregs(): array|false
```

Returns the retained multibyte regex captures from the last successful progressive search.

**Parameters**: none.

**Returns**: `array|false`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_ereg_search_getregs.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_ereg_search_getregs.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_ereg_search_getregs` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_ereg_search_getregs.md).
