---
title: "levenshtein()"
description: "Computes the Levenshtein edit distance between two strings."
sidebar:
  order: 951
---

## levenshtein()

```php
function levenshtein(string $string1, string $string2, int $insertion_cost = 1, int $replacement_cost = 1, int $deletion_cost = 1): int
```

Computes the Levenshtein edit distance between two strings.

**Parameters**:
- `$string1` (`string`)
- `$string2` (`string`)
- `$insertion_cost` (`int`), default `1`, optional
- `$replacement_cost` (`int`), default `1`, optional
- `$deletion_cost` (`int`), default `1`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected backend_gap prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/levenshtein.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/levenshtein.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `levenshtein` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/levenshtein.md).
