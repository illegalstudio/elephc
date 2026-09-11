---
title: "mb_ereg_search_init()"
description: "Initializes the retained multibyte regex subject and optionally compiles a search pattern."
sidebar:
  order: 816
---

## mb_ereg_search_init()

```php
function mb_ereg_search_init(string $string, ?string $pattern = null, ?string $options = null): bool
```

Initializes the retained multibyte regex subject and optionally compiles a search pattern.

**Parameters**:
- `$string` (`string`)
- `$pattern` (`?string`), default `null`, optional
- `$options` (`?string`), default `null`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_ereg_search_init.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_ereg_search_init.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_ereg_search_init` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_ereg_search_init.md).
