---
title: "Regex builtins"
description: "Builtins in the Regex category."
sidebar:
  order: 106
---

## Regex builtins

| Function | Signature | Returns |
|---|---|---|
| [`preg_match()`](./regex/preg_match.md) | `(string $pattern, string $subject, array $matches): int` | `int` |
| [`preg_match_all()`](./regex/preg_match_all.md) | `(string $pattern, string $subject, array $matches): int` | `int` |
| [`preg_replace()`](./regex/preg_replace.md) | `(string $pattern, string $replacement, string $subject, int $limit = -1, int $count = null): string` | `string` |
| [`preg_replace_callback()`](./regex/preg_replace_callback.md) | `(string $pattern, callable $callback, string $subject, int $limit = -1, int $count = null, int $flags = 0): array` | `array` |
| [`preg_split()`](./regex/preg_split.md) | `(string $pattern, string $subject, int $limit, int $flags): array` | `array` |
