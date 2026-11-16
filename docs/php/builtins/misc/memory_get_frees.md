---
title: "memory_get_frees()"
description: "Returns the cumulative number of native heap blocks released. Direct native reads do not allocate; eval boxes the sampled count."
---

## memory_get_frees()

```php
function memory_get_frees(): int
```

Returns the cumulative number of native heap blocks released. Direct native reads do not allocate; eval boxes the sampled count.

**Parameters**: none.

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Hidden in strict PHP mode: this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

**Examples**:

```php
$values = array_fill(0, 10000, 1);
$before = memory_get_frees();
unset($values);
$after = memory_get_frees();
echo $after - $before;
```

## Internals

For how `memory_get_frees` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/memory_get_frees.md).
