---
title: "mb_ereg_search_setpos()"
description: "Changes the progressive regex byte position, accepting negative offsets relative to the retained subject."
sidebar:
  order: 819
---

## mb_ereg_search_setpos()

```php
function mb_ereg_search_setpos(int $offset): bool
```

Changes the progressive regex byte position, accepting negative offsets relative to the retained subject.

**Parameters**:
- `$offset` (`int`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_ereg_search_setpos.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_ereg_search_setpos.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_ereg_search_setpos` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_ereg_search_setpos.md).
