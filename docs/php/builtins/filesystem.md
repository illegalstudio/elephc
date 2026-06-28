---
title: "Filesystem builtins"
description: "Builtins in the Filesystem category."
sidebar:
  order: 110
---

## Filesystem builtins

| Function | Signature | Returns |
|---|---|---|
| [`basename()`](./filesystem/basename.md) | `(string $path, string $suffix): string` | `string` |
| [`chdir()`](./filesystem/chdir.md) | `(string $directory): bool` | `bool` |
| [`chgrp()`](./filesystem/chgrp.md) | `(string $filename, int $group): bool` | `bool` |
| [`chmod()`](./filesystem/chmod.md) | `(string $filename, int $permissions): bool` | `bool` |
| [`chown()`](./filesystem/chown.md) | `(string $filename, int $user): bool` | `bool` |
| [`clearstatcache()`](./filesystem/clearstatcache.md) | `(bool $clear_realpath_cache, string $filename): void` | `void` |
| [`copy()`](./filesystem/copy.md) | `(string $from, string $to, mixed $context): bool` | `bool` |
| [`dirname()`](./filesystem/dirname.md) | `(string $path, int $levels): string` | `string` |
| [`disk_free_space()`](./filesystem/disk_free_space.md) | `(string $directory): float` | `float` |
| [`disk_total_space()`](./filesystem/disk_total_space.md) | `(string $directory): float` | `float` |
| [`file_exists()`](./filesystem/file_exists.md) | `(string $filename): bool` | `bool` |
| [`fileatime()`](./filesystem/fileatime.md) | `(string $filename): mixed` | `mixed` |
| [`filectime()`](./filesystem/filectime.md) | `(string $filename): mixed` | `mixed` |
| [`filegroup()`](./filesystem/filegroup.md) | `(string $filename): mixed` | `mixed` |
| [`fileinode()`](./filesystem/fileinode.md) | `(string $filename): mixed` | `mixed` |
| [`filemtime()`](./filesystem/filemtime.md) | `(string $filename): int` | `int` |
| [`fileowner()`](./filesystem/fileowner.md) | `(string $filename): mixed` | `mixed` |
| [`fileperms()`](./filesystem/fileperms.md) | `(string $filename): mixed` | `mixed` |
| [`filesize()`](./filesystem/filesize.md) | `(string $filename): int` | `int` |
| [`filetype()`](./filesystem/filetype.md) | `(string $filename): mixed` | `mixed` |
| [`fnmatch()`](./filesystem/fnmatch.md) | `(string $pattern, string $filename, int $flags): bool` | `bool` |
| [`getcwd()`](./filesystem/getcwd.md) | `(): string` | `string` |
| [`getenv()`](./filesystem/getenv.md) | `(string $name, bool $local_only): mixed` | `mixed` |
| [`glob()`](./filesystem/glob.md) | `(string $pattern, int $flags): array` | `array` |
| [`is_dir()`](./filesystem/is_dir.md) | `(string $filename): bool` | `bool` |
| [`is_executable()`](./filesystem/is_executable.md) | `(string $filename): bool` | `bool` |
| [`is_file()`](./filesystem/is_file.md) | `(string $filename): bool` | `bool` |
| [`is_link()`](./filesystem/is_link.md) | `(string $filename): bool` | `bool` |
| [`is_readable()`](./filesystem/is_readable.md) | `(string $filename): bool` | `bool` |
| [`is_writable()`](./filesystem/is_writable.md) | `(string $filename): bool` | `bool` |
| [`is_writeable()`](./filesystem/is_writeable.md) | `(string $filename): bool` | `bool` |
| [`lchgrp()`](./filesystem/lchgrp.md) | `(string $filename, int $group): bool` | `bool` |
| [`lchown()`](./filesystem/lchown.md) | `(string $filename, int $user): bool` | `bool` |
| [`link()`](./filesystem/link.md) | `(string $target, string $link): bool` | `bool` |
| [`linkinfo()`](./filesystem/linkinfo.md) | `(string $path): int` | `int` |
| [`lstat()`](./filesystem/lstat.md) | `(string $filename): mixed` | `mixed` |
| [`mkdir()`](./filesystem/mkdir.md) | `(string $directory, int $permissions, bool $recursive, bool $context): bool` | `bool` |
| [`pathinfo()`](./filesystem/pathinfo.md) | `(string $path, int $flags): mixed` | `mixed` |
| [`putenv()`](./filesystem/putenv.md) | `(string $assignment): bool` | `bool` |
| [`readfile()`](./filesystem/readfile.md) | `(string $filename, bool $use_include_path, mixed $context): mixed` | `mixed` |
| [`readlink()`](./filesystem/readlink.md) | `(string $path): mixed` | `mixed` |
| [`realpath()`](./filesystem/realpath.md) | `(string $path): mixed` | `mixed` |
| [`realpath_cache_get()`](./filesystem/realpath_cache_get.md) | `(): array` | `array` |
| [`realpath_cache_size()`](./filesystem/realpath_cache_size.md) | `(): int` | `int` |
| [`rename()`](./filesystem/rename.md) | `(string $from, string $to, mixed $context): bool` | `bool` |
| [`rmdir()`](./filesystem/rmdir.md) | `(string $directory, mixed $context = null): bool` | `bool` |
| [`scandir()`](./filesystem/scandir.md) | `(string $directory, int $sorting_order, mixed $context): array` | `array` |
| [`stat()`](./filesystem/stat.md) | `(string $filename): mixed` | `mixed` |
| [`symlink()`](./filesystem/symlink.md) | `(string $target, string $link): bool` | `bool` |
| [`sys_get_temp_dir()`](./filesystem/sys_get_temp_dir.md) | `(): string` | `string` |
| [`tempnam()`](./filesystem/tempnam.md) | `(string $directory, string $prefix): string` | `string` |
| [`tmpfile()`](./filesystem/tmpfile.md) | `(): mixed` | `mixed` |
| [`touch()`](./filesystem/touch.md) | `(string $filename, int $mtime, int $atime): bool` | `bool` |
| [`umask()`](./filesystem/umask.md) | `(int $mask): int` | `int` |
| [`unlink()`](./filesystem/unlink.md) | `(string $filename): bool` | `bool` |
