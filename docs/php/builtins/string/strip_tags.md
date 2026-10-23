---
title: "strip_tags()"
description: "Strips HTML and PHP tags from a string. Optional `$allowed_tags` is a string like `\"<p><a>\"` or an array of tag names. Weak callers also coerce scalars and Stringable objects; strict callers require `array|string|null`."
sidebar:
  order: 928
---

## strip_tags()

```php
function strip_tags(string $string, array|string|null $allowed_tags = null): string
```

Strips HTML and PHP tags from a string. Optional `$allowed_tags` is a string like `"<p><a>"` or an array of tag names. Weak callers also coerce scalars and Stringable objects; strict callers require `array|string|null`.

**Parameters**:
- `$string` (`string`)
- `$allowed_tags` (`array|string|null`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/strip_tags.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/strip_tags.rs)).

**Examples**:

```php
echo strip_tags("<p>Hello <b>World</b></p>");
```

```php
echo strip_tags("<p>Hello <b>World</b></p>", "<p>");
```

```php
echo strip_tags("<p>Hello <b>World</b></p>", ["p"]);
```

## Internals

For how `strip_tags` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strip_tags.md).
