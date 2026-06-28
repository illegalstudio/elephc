---
title: "Builtins"
description: "Index of all PHP builtins supported by Elephc."
sidebar:
  order: 0
---

## Builtins

| Function | Signature | Returns |
|---|---|---|
| [`array_chunk()`](./builtins/array/array_chunk.md) | `(array $array, int $length, bool $preserve_keys): array` | `array` |
| [`array_column()`](./builtins/array/array_column.md) | `(array $array, string $column_key, string $index_key): array` | `array` |
| [`array_combine()`](./builtins/array/array_combine.md) | `(array $keys, array $values): array` | `array` |
| [`array_diff()`](./builtins/array/array_diff.md) | `(array $array, ...$arrays): array` | `array` |
| [`array_diff_key()`](./builtins/array/array_diff_key.md) | `(array $array, ...$arrays): array` | `array` |
| [`array_fill()`](./builtins/array/array_fill.md) | `(int $start_index, int $count, mixed $value): array` | `array` |
| [`array_fill_keys()`](./builtins/array/array_fill_keys.md) | `(array $keys, mixed $value): array` | `array` |
| [`array_filter()`](./builtins/array/array_filter.md) | `(array $array, callable $callback, int $mode): array` | `array` |
| [`array_flip()`](./builtins/array/array_flip.md) | `(array $array): float` | `float` |
| [`array_intersect()`](./builtins/array/array_intersect.md) | `(array $array, ...$arrays): array` | `array` |
| [`array_intersect_key()`](./builtins/array/array_intersect_key.md) | `(array $array, ...$arrays): array` | `array` |
| [`array_key_exists()`](./builtins/array/array_key_exists.md) | `(string $key, array $array): bool` | `bool` |
| [`array_keys()`](./builtins/array/array_keys.md) | `(array $array, string $filter_value, bool $strict): array` | `array` |
| [`array_map()`](./builtins/array/array_map.md) | `(callable $callback, array $array, ...$arrays): array` | `array` |
| [`array_merge()`](./builtins/array/array_merge.md) | `(...$arrays): array` | `array` |
| [`array_pad()`](./builtins/array/array_pad.md) | `(array $array, int $length, mixed $value): array` | `array` |
| [`array_pop()`](./builtins/array/array_pop.md) | `(array $array): mixed` | `mixed` |
| [`array_product()`](./builtins/array/array_product.md) | `(array $array): float` | `float` |
| [`array_push()`](./builtins/array/array_push.md) | `(array $array, ...$values): void` | `void` |
| [`array_rand()`](./builtins/array/array_rand.md) | `(array $array, int $num): int` | `int` |
| [`array_reduce()`](./builtins/array/array_reduce.md) | `(array $array, callable $callback, mixed $initial): int` | `int` |
| [`array_reverse()`](./builtins/array/array_reverse.md) | `(array $array, bool $preserve_keys): array` | `array` |
| [`array_search()`](./builtins/array/array_search.md) | `(mixed $needle, array $haystack, bool $strict): mixed` | `mixed` |
| [`array_shift()`](./builtins/array/array_shift.md) | `(array $array): mixed` | `mixed` |
| [`array_slice()`](./builtins/array/array_slice.md) | `(array $array, int $offset, int $length, bool $preserve_keys): array` | `array` |
| [`array_splice()`](./builtins/array/array_splice.md) | `(array $array, int $offset, int $length, array $replacement): array` | `array` |
| [`array_sum()`](./builtins/array/array_sum.md) | `(array $array): float` | `float` |
| [`array_unique()`](./builtins/array/array_unique.md) | `(array $array, int $flags): array` | `array` |
| [`array_unshift()`](./builtins/array/array_unshift.md) | `(array $array, ...$values): int` | `int` |
| [`array_values()`](./builtins/array/array_values.md) | `(array $array): array` | `array` |
| [`array_walk()`](./builtins/array/array_walk.md) | `(array $array, callable $callback, mixed $arg): void` | `void` |
| [`arsort()`](./builtins/array/arsort.md) | `(array $array, int $flags): bool` | `bool` |
| [`asort()`](./builtins/array/asort.md) | `(array $array, int $flags): bool` | `bool` |
| [`count()`](./builtins/array/count.md) | `(array $value, int $mode): int` | `int` |
| [`in_array()`](./builtins/array/in_array.md) | `(mixed $needle, array $haystack, bool $strict): mixed` | `mixed` |
| [`krsort()`](./builtins/array/krsort.md) | `(array $array, int $flags): bool` | `bool` |
| [`ksort()`](./builtins/array/ksort.md) | `(array $array, int $flags): bool` | `bool` |
| [`natcasesort()`](./builtins/array/natcasesort.md) | `(array $array): bool` | `bool` |
| [`natsort()`](./builtins/array/natsort.md) | `(array $array): bool` | `bool` |
| [`range()`](./builtins/array/range.md) | `(mixed $start, mixed $end, int $step): array` | `array` |
| [`rsort()`](./builtins/array/rsort.md) | `(array $array, int $flags): bool` | `bool` |
| [`shuffle()`](./builtins/array/shuffle.md) | `(array $array): bool` | `bool` |
| [`sort()`](./builtins/array/sort.md) | `(array $array, int $flags): bool` | `bool` |
| [`uasort()`](./builtins/array/uasort.md) | `(array $array, callable $callback): bool` | `bool` |
| [`uksort()`](./builtins/array/uksort.md) | `(array $array, callable $callback): bool` | `bool` |
| [`usort()`](./builtins/array/usort.md) | `(array $array, callable $callback): bool` | `bool` |
| [`buffer_free()`](./builtins/buffer/buffer_free.md) | `(buffer $buffer): mixed` | `mixed` |
| [`buffer_len()`](./builtins/buffer/buffer_len.md) | `(buffer $buffer): int` | `int` |
| [`class_alias()`](./builtins/class/class_alias.md) | `(string $class, string $alias, bool $autoload): bool` | `bool` |
| [`class_attribute_args()`](./builtins/class/class_attribute_args.md) | `(string $class_name, string $attribute_name): array` | `array` |
| [`class_attribute_names()`](./builtins/class/class_attribute_names.md) | `(string $class_name): array` | `array` |
| [`class_exists()`](./builtins/class/class_exists.md) | `(string $class, bool $autoload): bool` | `bool` |
| [`class_get_attributes()`](./builtins/class/class_get_attributes.md) | `(string $class_name): mixed` | `mixed` |
| [`class_implements()`](./builtins/class/class_implements.md) | `(mixed $object_or_class, bool $autoload): mixed` | `mixed` |
| [`class_parents()`](./builtins/class/class_parents.md) | `(mixed $object_or_class, bool $autoload): mixed` | `mixed` |
| [`class_uses()`](./builtins/class/class_uses.md) | `(mixed $object_or_class, bool $autoload): mixed` | `mixed` |
| [`enum_exists()`](./builtins/class/enum_exists.md) | `(string $enum, bool $autoload): bool` | `bool` |
| [`function_exists()`](./builtins/class/function_exists.md) | `(string $function): bool` | `bool` |
| [`get_class()`](./builtins/class/get_class.md) | `(object $object): string` | `string` |
| [`get_declared_classes()`](./builtins/class/get_declared_classes.md) | `(): array` | `array` |
| [`get_declared_interfaces()`](./builtins/class/get_declared_interfaces.md) | `(): array` | `array` |
| [`get_declared_traits()`](./builtins/class/get_declared_traits.md) | `(): array` | `array` |
| [`get_parent_class()`](./builtins/class/get_parent_class.md) | `(mixed $object_or_class): string` | `string` |
| [`interface_exists()`](./builtins/class/interface_exists.md) | `(string $interface, bool $autoload): bool` | `bool` |
| [`is_a()`](./builtins/class/is_a.md) | `(object $object_or_class, string $class, bool $allow_string): bool` | `bool` |
| [`is_subclass_of()`](./builtins/class/is_subclass_of.md) | `(mixed $object_or_class, string $class, bool $allow_string): bool` | `bool` |
| [`trait_exists()`](./builtins/class/trait_exists.md) | `(string $trait, bool $autoload): bool` | `bool` |
| [`checkdate()`](./builtins/date/checkdate.md) | `(int $month, int $day, int $year): bool` | `bool` |
| [`date()`](./builtins/date/date.md) | `(string $format, int $timestamp): string` | `string` |
| [`date_default_timezone_get()`](./builtins/date/date_default_timezone_get.md) | `(): string` | `string` |
| [`date_default_timezone_set()`](./builtins/date/date_default_timezone_set.md) | `(string $timezoneId): bool` | `bool` |
| [`getdate()`](./builtins/date/getdate.md) | `(int $timestamp): array` | `array` |
| [`gmdate()`](./builtins/date/gmdate.md) | `(string $format, int $timestamp): string` | `string` |
| [`gmmktime()`](./builtins/date/gmmktime.md) | `(int $hour, int $minute, int $second, int $month, int $day, int $year): int` | `int` |
| [`hrtime()`](./builtins/date/hrtime.md) | `(bool $as_number): mixed` | `mixed` |
| [`localtime()`](./builtins/date/localtime.md) | `(int $timestamp, bool $associative): array` | `array` |
| [`microtime()`](./builtins/date/microtime.md) | `(bool $as_float): int` | `int` |
| [`mktime()`](./builtins/date/mktime.md) | `(int $hour, int $minute, int $second, int $month, int $day, int $year): int` | `int` |
| [`strtotime()`](./builtins/date/strtotime.md) | `(string $datetime, int $baseTimestamp): mixed` | `mixed` |
| [`time()`](./builtins/date/time.md) | `(): int` | `int` |
| [`basename()`](./builtins/filesystem/basename.md) | `(string $path, string $suffix): string` | `string` |
| [`chdir()`](./builtins/filesystem/chdir.md) | `(string $directory): bool` | `bool` |
| [`chgrp()`](./builtins/filesystem/chgrp.md) | `(string $filename, int $group): bool` | `bool` |
| [`chmod()`](./builtins/filesystem/chmod.md) | `(string $filename, int $permissions): bool` | `bool` |
| [`chown()`](./builtins/filesystem/chown.md) | `(string $filename, int $user): bool` | `bool` |
| [`clearstatcache()`](./builtins/filesystem/clearstatcache.md) | `(bool $clear_realpath_cache, string $filename): void` | `void` |
| [`copy()`](./builtins/filesystem/copy.md) | `(string $from, string $to, mixed $context): bool` | `bool` |
| [`dirname()`](./builtins/filesystem/dirname.md) | `(string $path, int $levels): string` | `string` |
| [`disk_free_space()`](./builtins/filesystem/disk_free_space.md) | `(string $directory): float` | `float` |
| [`disk_total_space()`](./builtins/filesystem/disk_total_space.md) | `(string $directory): float` | `float` |
| [`file_exists()`](./builtins/filesystem/file_exists.md) | `(string $filename): bool` | `bool` |
| [`fileatime()`](./builtins/filesystem/fileatime.md) | `(string $filename): mixed` | `mixed` |
| [`filectime()`](./builtins/filesystem/filectime.md) | `(string $filename): mixed` | `mixed` |
| [`filegroup()`](./builtins/filesystem/filegroup.md) | `(string $filename): mixed` | `mixed` |
| [`fileinode()`](./builtins/filesystem/fileinode.md) | `(string $filename): mixed` | `mixed` |
| [`filemtime()`](./builtins/filesystem/filemtime.md) | `(string $filename): int` | `int` |
| [`fileowner()`](./builtins/filesystem/fileowner.md) | `(string $filename): mixed` | `mixed` |
| [`fileperms()`](./builtins/filesystem/fileperms.md) | `(string $filename): mixed` | `mixed` |
| [`filesize()`](./builtins/filesystem/filesize.md) | `(string $filename): int` | `int` |
| [`filetype()`](./builtins/filesystem/filetype.md) | `(string $filename): mixed` | `mixed` |
| [`fnmatch()`](./builtins/filesystem/fnmatch.md) | `(string $pattern, string $filename, int $flags): bool` | `bool` |
| [`getcwd()`](./builtins/filesystem/getcwd.md) | `(): string` | `string` |
| [`getenv()`](./builtins/filesystem/getenv.md) | `(string $name, bool $local_only): mixed` | `mixed` |
| [`glob()`](./builtins/filesystem/glob.md) | `(string $pattern, int $flags): array` | `array` |
| [`is_dir()`](./builtins/filesystem/is_dir.md) | `(string $filename): bool` | `bool` |
| [`is_executable()`](./builtins/filesystem/is_executable.md) | `(string $filename): bool` | `bool` |
| [`is_file()`](./builtins/filesystem/is_file.md) | `(string $filename): bool` | `bool` |
| [`is_link()`](./builtins/filesystem/is_link.md) | `(string $filename): bool` | `bool` |
| [`is_readable()`](./builtins/filesystem/is_readable.md) | `(string $filename): bool` | `bool` |
| [`is_writable()`](./builtins/filesystem/is_writable.md) | `(string $filename): bool` | `bool` |
| [`is_writeable()`](./builtins/filesystem/is_writeable.md) | `(string $filename): bool` | `bool` |
| [`lchgrp()`](./builtins/filesystem/lchgrp.md) | `(string $filename, int $group): bool` | `bool` |
| [`lchown()`](./builtins/filesystem/lchown.md) | `(string $filename, int $user): bool` | `bool` |
| [`link()`](./builtins/filesystem/link.md) | `(string $target, string $link): bool` | `bool` |
| [`linkinfo()`](./builtins/filesystem/linkinfo.md) | `(string $path): int` | `int` |
| [`lstat()`](./builtins/filesystem/lstat.md) | `(string $filename): mixed` | `mixed` |
| [`mkdir()`](./builtins/filesystem/mkdir.md) | `(string $directory, int $permissions, bool $recursive, bool $context): bool` | `bool` |
| [`pathinfo()`](./builtins/filesystem/pathinfo.md) | `(string $path, int $flags): mixed` | `mixed` |
| [`putenv()`](./builtins/filesystem/putenv.md) | `(string $assignment): bool` | `bool` |
| [`readfile()`](./builtins/filesystem/readfile.md) | `(string $filename, bool $use_include_path, mixed $context): mixed` | `mixed` |
| [`readlink()`](./builtins/filesystem/readlink.md) | `(string $path): mixed` | `mixed` |
| [`realpath()`](./builtins/filesystem/realpath.md) | `(string $path): mixed` | `mixed` |
| [`realpath_cache_get()`](./builtins/filesystem/realpath_cache_get.md) | `(): array` | `array` |
| [`realpath_cache_size()`](./builtins/filesystem/realpath_cache_size.md) | `(): int` | `int` |
| [`rename()`](./builtins/filesystem/rename.md) | `(string $from, string $to, mixed $context): bool` | `bool` |
| [`rmdir()`](./builtins/filesystem/rmdir.md) | `(string $directory, mixed $context = null): bool` | `bool` |
| [`scandir()`](./builtins/filesystem/scandir.md) | `(string $directory, int $sorting_order, mixed $context): array` | `array` |
| [`stat()`](./builtins/filesystem/stat.md) | `(string $filename): mixed` | `mixed` |
| [`symlink()`](./builtins/filesystem/symlink.md) | `(string $target, string $link): bool` | `bool` |
| [`sys_get_temp_dir()`](./builtins/filesystem/sys_get_temp_dir.md) | `(): string` | `string` |
| [`tempnam()`](./builtins/filesystem/tempnam.md) | `(string $directory, string $prefix): string` | `string` |
| [`tmpfile()`](./builtins/filesystem/tmpfile.md) | `(): mixed` | `mixed` |
| [`touch()`](./builtins/filesystem/touch.md) | `(string $filename, int $mtime, int $atime): bool` | `bool` |
| [`umask()`](./builtins/filesystem/umask.md) | `(int $mask): int` | `int` |
| [`unlink()`](./builtins/filesystem/unlink.md) | `(string $filename): bool` | `bool` |
| [`closedir()`](./builtins/io/closedir.md) | `(resource $dir_handle): void` | `void` |
| [`fclose()`](./builtins/io/fclose.md) | `(resource $stream): bool` | `bool` |
| [`fdatasync()`](./builtins/io/fdatasync.md) | `(resource $stream): bool` | `bool` |
| [`feof()`](./builtins/io/feof.md) | `(resource $stream): bool` | `bool` |
| [`fflush()`](./builtins/io/fflush.md) | `(resource $stream): bool` | `bool` |
| [`fgetc()`](./builtins/io/fgetc.md) | `(resource $stream): mixed` | `mixed` |
| [`fgetcsv()`](./builtins/io/fgetcsv.md) | `(resource $stream, int $length, string $separator, string $enclosure, string $escape): array` | `array` |
| [`fgets()`](./builtins/io/fgets.md) | `(resource $stream, int $length): mixed` | `mixed` |
| [`file()`](./builtins/io/file.md) | `(string $filename, int $flags, mixed $context): array` | `array` |
| [`file_get_contents()`](./builtins/io/file_get_contents.md) | `(string $filename, bool $use_include_path, mixed $context, int $offset, int $length): mixed` | `mixed` |
| [`file_put_contents()`](./builtins/io/file_put_contents.md) | `(string $filename, mixed $data, int $flags = 0, mixed $context = null): int` | `int` |
| [`flock()`](./builtins/io/flock.md) | `(resource $stream, int $operation, bool $would_block): bool` | `bool` |
| [`fopen()`](./builtins/io/fopen.md) | `(string $filename, string $mode, bool $use_include_path, mixed $context): mixed` | `mixed` |
| [`fpassthru()`](./builtins/io/fpassthru.md) | `(resource $stream): int` | `int` |
| [`fprintf()`](./builtins/io/fprintf.md) | `(resource $stream, string $format, ...$values): int` | `int` |
| [`fputcsv()`](./builtins/io/fputcsv.md) | `(resource $stream, array $fields, string $separator = ',', string $enclosure = '"', string $escape = '\\', string $eol = '\n'): int` | `int` |
| [`fread()`](./builtins/io/fread.md) | `(resource $stream, int $length): string` | `string` |
| [`fscanf()`](./builtins/io/fscanf.md) | `(resource $stream, string $format, ...$vars): array` | `array` |
| [`fseek()`](./builtins/io/fseek.md) | `(resource $stream, int $offset, int $whence): int` | `int` |
| [`fstat()`](./builtins/io/fstat.md) | `(resource $stream): mixed` | `mixed` |
| [`fsync()`](./builtins/io/fsync.md) | `(resource $stream): bool` | `bool` |
| [`ftell()`](./builtins/io/ftell.md) | `(resource $stream): int` | `int` |
| [`ftruncate()`](./builtins/io/ftruncate.md) | `(resource $stream, int $size): bool` | `bool` |
| [`fwrite()`](./builtins/io/fwrite.md) | `(resource $stream, string $data, int $length): int` | `int` |
| [`gethostbyaddr()`](./builtins/io/gethostbyaddr.md) | `(string $ip): mixed` | `mixed` |
| [`gethostbyname()`](./builtins/io/gethostbyname.md) | `(string $hostname): string` | `string` |
| [`gethostname()`](./builtins/io/gethostname.md) | `(): string` | `string` |
| [`getprotobyname()`](./builtins/io/getprotobyname.md) | `(string $protocol): mixed` | `mixed` |
| [`getprotobynumber()`](./builtins/io/getprotobynumber.md) | `(int $protocol): mixed` | `mixed` |
| [`getservbyname()`](./builtins/io/getservbyname.md) | `(string $service, string $protocol): mixed` | `mixed` |
| [`getservbyport()`](./builtins/io/getservbyport.md) | `(int $port, string $protocol): mixed` | `mixed` |
| [`hash_file()`](./builtins/io/hash_file.md) | `(string $algo, string $filename, bool $binary = false, array $options = []): mixed` | `mixed` |
| [`opendir()`](./builtins/io/opendir.md) | `(string $directory): mixed` | `mixed` |
| [`readdir()`](./builtins/io/readdir.md) | `(resource $dir_handle): mixed` | `mixed` |
| [`rewind()`](./builtins/io/rewind.md) | `(resource $stream): bool` | `bool` |
| [`rewinddir()`](./builtins/io/rewinddir.md) | `(resource $dir_handle): void` | `void` |
| [`stream_bucket_make_writeable()`](./builtins/io/stream_bucket_make_writeable.md) | `(mixed $brigade): mixed` | `mixed` |
| [`stream_bucket_new()`](./builtins/io/stream_bucket_new.md) | `(resource $stream, string $buffer): mixed` | `mixed` |
| [`stream_context_create()`](./builtins/io/stream_context_create.md) | `(array $options, array $params): mixed` | `mixed` |
| [`stream_context_get_default()`](./builtins/io/stream_context_get_default.md) | `(array $options): mixed` | `mixed` |
| [`stream_context_get_options()`](./builtins/io/stream_context_get_options.md) | `(resource $stream_or_context): array` | `array` |
| [`stream_context_get_params()`](./builtins/io/stream_context_get_params.md) | `(resource $context): array` | `array` |
| [`stream_context_set_default()`](./builtins/io/stream_context_set_default.md) | `(array $options): mixed` | `mixed` |
| [`stream_context_set_option()`](./builtins/io/stream_context_set_option.md) | `(resource $context, string $wrapper_or_options, string $option_name, mixed $value): bool` | `bool` |
| [`stream_context_set_params()`](./builtins/io/stream_context_set_params.md) | `(resource $context, array $params): bool` | `bool` |
| [`stream_copy_to_stream()`](./builtins/io/stream_copy_to_stream.md) | `(resource $from, resource $to, int $length, int $offset): mixed` | `mixed` |
| [`stream_filter_register()`](./builtins/io/stream_filter_register.md) | `(string $filter_name, string $class): bool` | `bool` |
| [`stream_filter_remove()`](./builtins/io/stream_filter_remove.md) | `(resource $stream_filter): bool` | `bool` |
| [`stream_get_contents()`](./builtins/io/stream_get_contents.md) | `(resource $stream, int $length, int $offset): mixed` | `mixed` |
| [`stream_get_filters()`](./builtins/io/stream_get_filters.md) | `(): array` | `array` |
| [`stream_get_line()`](./builtins/io/stream_get_line.md) | `(resource $stream, int $length, string $ending): string` | `string` |
| [`stream_get_meta_data()`](./builtins/io/stream_get_meta_data.md) | `(resource $stream): array` | `array` |
| [`stream_get_transports()`](./builtins/io/stream_get_transports.md) | `(): array` | `array` |
| [`stream_get_wrappers()`](./builtins/io/stream_get_wrappers.md) | `(): array` | `array` |
| [`stream_is_local()`](./builtins/io/stream_is_local.md) | `(resource $stream): bool` | `bool` |
| [`stream_isatty()`](./builtins/io/stream_isatty.md) | `(resource $stream): bool` | `bool` |
| [`stream_resolve_include_path()`](./builtins/io/stream_resolve_include_path.md) | `(string $filename): mixed` | `mixed` |
| [`stream_select()`](./builtins/io/stream_select.md) | `(array $read, array $write, array $except, int $seconds, int $microseconds): int` | `int` |
| [`stream_set_blocking()`](./builtins/io/stream_set_blocking.md) | `(resource $stream, bool $enable): bool` | `bool` |
| [`stream_set_chunk_size()`](./builtins/io/stream_set_chunk_size.md) | `(resource $stream, int $size): int` | `int` |
| [`stream_set_read_buffer()`](./builtins/io/stream_set_read_buffer.md) | `(resource $stream, int $size): int` | `int` |
| [`stream_set_timeout()`](./builtins/io/stream_set_timeout.md) | `(resource $stream, int $seconds, int $microseconds): bool` | `bool` |
| [`stream_set_write_buffer()`](./builtins/io/stream_set_write_buffer.md) | `(resource $stream, int $size): int` | `int` |
| [`stream_socket_accept()`](./builtins/io/stream_socket_accept.md) | `(resource $socket, float $timeout, string $peer_name): mixed` | `mixed` |
| [`stream_socket_client()`](./builtins/io/stream_socket_client.md) | `(string $address, int $error_code, int $error_message, string $timeout, float $flags): mixed` | `mixed` |
| [`stream_socket_enable_crypto()`](./builtins/io/stream_socket_enable_crypto.md) | `(resource $stream, bool $enable, int $crypto_method, resource $session_stream): bool` | `bool` |
| [`stream_socket_get_name()`](./builtins/io/stream_socket_get_name.md) | `(resource $socket, bool $remote): mixed` | `mixed` |
| [`stream_socket_pair()`](./builtins/io/stream_socket_pair.md) | `(int $domain, int $type, int $protocol): mixed` | `mixed` |
| [`stream_socket_recvfrom()`](./builtins/io/stream_socket_recvfrom.md) | `(resource $socket, int $length, int $flags, string $address): mixed` | `mixed` |
| [`stream_socket_sendto()`](./builtins/io/stream_socket_sendto.md) | `(resource $socket, string $data, int $flags, string $address): mixed` | `mixed` |
| [`stream_socket_server()`](./builtins/io/stream_socket_server.md) | `(string $address, int $error_code, int $error_message): mixed` | `mixed` |
| [`stream_socket_shutdown()`](./builtins/io/stream_socket_shutdown.md) | `(resource $stream, int $mode): bool` | `bool` |
| [`stream_supports_lock()`](./builtins/io/stream_supports_lock.md) | `(resource $stream): bool` | `bool` |
| [`stream_wrapper_register()`](./builtins/io/stream_wrapper_register.md) | `(string $protocol, string $class, int $flags): bool` | `bool` |
| [`stream_wrapper_restore()`](./builtins/io/stream_wrapper_restore.md) | `(string $protocol): bool` | `bool` |
| [`stream_wrapper_unregister()`](./builtins/io/stream_wrapper_unregister.md) | `(string $protocol): bool` | `bool` |
| [`vfprintf()`](./builtins/io/vfprintf.md) | `(resource $stream, string $format, array $values): int` | `int` |
| [`json_decode()`](./builtins/json/json_decode.md) | `(string $json, bool $associative, int $depth, int $flags): mixed` | `mixed` |
| [`json_encode()`](./builtins/json/json_encode.md) | `(mixed $value, int $flags, int $depth): string` | `string` |
| [`json_last_error()`](./builtins/json/json_last_error.md) | `(): int` | `int` |
| [`json_last_error_msg()`](./builtins/json/json_last_error_msg.md) | `(): string` | `string` |
| [`json_validate()`](./builtins/json/json_validate.md) | `(string $json, int $depth, int $flags): bool` | `bool` |
| [`abs()`](./builtins/math/abs.md) | `(int $num): mixed` | `mixed` |
| [`acos()`](./builtins/math/acos.md) | `(float $num): float` | `float` |
| [`asin()`](./builtins/math/asin.md) | `(float $num): float` | `float` |
| [`atan()`](./builtins/math/atan.md) | `(float $num): float` | `float` |
| [`atan2()`](./builtins/math/atan2.md) | `(float $y, float $x): float` | `float` |
| [`ceil()`](./builtins/math/ceil.md) | `(float $num): float` | `float` |
| [`clamp()`](./builtins/math/clamp.md) | `(int $value, int $min, int $max): string` | `string` |
| [`cos()`](./builtins/math/cos.md) | `(float $num): float` | `float` |
| [`cosh()`](./builtins/math/cosh.md) | `(float $num): float` | `float` |
| [`deg2rad()`](./builtins/math/deg2rad.md) | `(float $num): float` | `float` |
| [`exp()`](./builtins/math/exp.md) | `(float $num): float` | `float` |
| [`fdiv()`](./builtins/math/fdiv.md) | `(float $num1, float $num2): float` | `float` |
| [`floor()`](./builtins/math/floor.md) | `(float $num): float` | `float` |
| [`fmod()`](./builtins/math/fmod.md) | `(float $num1, float $num2): float` | `float` |
| [`hypot()`](./builtins/math/hypot.md) | `(float $x, float $y): float` | `float` |
| [`intdiv()`](./builtins/math/intdiv.md) | `(int $num1, int $num2): int` | `int` |
| [`is_finite()`](./builtins/math/is_finite.md) | `(float $num): bool` | `bool` |
| [`is_infinite()`](./builtins/math/is_infinite.md) | `(float $num): bool` | `bool` |
| [`is_nan()`](./builtins/math/is_nan.md) | `(float $num): bool` | `bool` |
| [`log()`](./builtins/math/log.md) | `(float $num, float $base): float` | `float` |
| [`log10()`](./builtins/math/log10.md) | `(float $num): float` | `float` |
| [`log2()`](./builtins/math/log2.md) | `(float $num): float` | `float` |
| [`max()`](./builtins/math/max.md) | `(mixed $value, ...$values): float` | `float` |
| [`min()`](./builtins/math/min.md) | `(mixed $value, ...$values): float` | `float` |
| [`mt_rand()`](./builtins/math/mt_rand.md) | `(int $min, int $max): int` | `int` |
| [`pi()`](./builtins/math/pi.md) | `(): float` | `float` |
| [`pow()`](./builtins/math/pow.md) | `(float $num, float $exponent): float` | `float` |
| [`rad2deg()`](./builtins/math/rad2deg.md) | `(float $num): float` | `float` |
| [`rand()`](./builtins/math/rand.md) | `(int $min, int $max): int` | `int` |
| [`random_int()`](./builtins/math/random_int.md) | `(int $min, int $max): int` | `int` |
| [`round()`](./builtins/math/round.md) | `(float $num, int $precision): float` | `float` |
| [`sin()`](./builtins/math/sin.md) | `(float $num): float` | `float` |
| [`sinh()`](./builtins/math/sinh.md) | `(float $num): float` | `float` |
| [`sqrt()`](./builtins/math/sqrt.md) | `(float $num): float` | `float` |
| [`tan()`](./builtins/math/tan.md) | `(float $num): float` | `float` |
| [`tanh()`](./builtins/math/tanh.md) | `(float $num): float` | `float` |
| [`buffer_new()`](./builtins/misc/buffer_new.md) | `(int $length): mixed` | `mixed` |
| [`call_user_func()`](./builtins/misc/call_user_func.md) | `(callable $callback, ...$args): mixed` | `mixed` |
| [`call_user_func_array()`](./builtins/misc/call_user_func_array.md) | `(callable $callback, array $args): mixed` | `mixed` |
| [`define()`](./builtins/misc/define.md) | `(string $constant_name, mixed $value, bool $case_insensitive): bool` | `bool` |
| [`defined()`](./builtins/misc/defined.md) | `(string $constant_name): bool` | `bool` |
| [`empty()`](./builtins/misc/empty.md) | `(mixed $value): bool` | `bool` |
| [`header()`](./builtins/misc/header.md) | `(mixed $header, mixed $replace, mixed $response_code): void` | `void` |
| [`http_response_code()`](./builtins/misc/http_response_code.md) | `(mixed $response_code): int` | `int` |
| [`isset()`](./builtins/misc/isset.md) | `(mixed $var, ...$vars): int` | `int` |
| [`php_uname()`](./builtins/misc/php_uname.md) | `(string $mode): string` | `string` |
| [`phpversion()`](./builtins/misc/phpversion.md) | `(string $extension = null): string` | `string` |
| [`print_r()`](./builtins/misc/print_r.md) | `(...$values): void` | `void` |
| [`unset()`](./builtins/misc/unset.md) | `(mixed $var, ...$vars): void` | `void` |
| [`var_dump()`](./builtins/misc/var_dump.md) | `(...$values): void` | `void` |
| [`ptr()`](./builtins/pointer/ptr.md) | `(mixed $value): mixed` | `mixed` |
| [`ptr_get()`](./builtins/pointer/ptr_get.md) | `(pointer $pointer): int` | `int` |
| [`ptr_is_null()`](./builtins/pointer/ptr_is_null.md) | `(pointer $pointer): bool` | `bool` |
| [`ptr_null()`](./builtins/pointer/ptr_null.md) | `(): mixed` | `mixed` |
| [`ptr_offset()`](./builtins/pointer/ptr_offset.md) | `(pointer $pointer, int $offset): mixed` | `mixed` |
| [`ptr_read16()`](./builtins/pointer/ptr_read16.md) | `(pointer $pointer): int` | `int` |
| [`ptr_read32()`](./builtins/pointer/ptr_read32.md) | `(pointer $pointer): int` | `int` |
| [`ptr_read8()`](./builtins/pointer/ptr_read8.md) | `(pointer $pointer): int` | `int` |
| [`ptr_read_string()`](./builtins/pointer/ptr_read_string.md) | `(pointer $pointer, int $length): string` | `string` |
| [`ptr_set()`](./builtins/pointer/ptr_set.md) | `(pointer $pointer, mixed $value): void` | `void` |
| [`ptr_sizeof()`](./builtins/pointer/ptr_sizeof.md) | `(string $type): mixed` | `mixed` |
| [`ptr_write16()`](./builtins/pointer/ptr_write16.md) | `(pointer $pointer, int $value): void` | `void` |
| [`ptr_write32()`](./builtins/pointer/ptr_write32.md) | `(pointer $pointer, int $value): void` | `void` |
| [`ptr_write8()`](./builtins/pointer/ptr_write8.md) | `(pointer $pointer, int $value): void` | `void` |
| [`ptr_write_string()`](./builtins/pointer/ptr_write_string.md) | `(pointer $pointer, string $string): int` | `int` |
| [`die()`](./builtins/process/die.md) | `(int $status): void` | `void` |
| [`exec()`](./builtins/process/exec.md) | `(string $command, array $output, int $result_code): string` | `string` |
| [`exit()`](./builtins/process/exit.md) | `(int $status): void` | `void` |
| [`passthru()`](./builtins/process/passthru.md) | `(string $command, int $result_code): void` | `void` |
| [`pclose()`](./builtins/process/pclose.md) | `(resource $handle): int` | `int` |
| [`popen()`](./builtins/process/popen.md) | `(string $command, string $mode): mixed` | `mixed` |
| [`readline()`](./builtins/process/readline.md) | `(string $prompt): mixed` | `mixed` |
| [`shell_exec()`](./builtins/process/shell_exec.md) | `(string $command): string` | `string` |
| [`sleep()`](./builtins/process/sleep.md) | `(int $seconds): int` | `int` |
| [`system()`](./builtins/process/system.md) | `(string $command, int $result_code): string` | `string` |
| [`usleep()`](./builtins/process/usleep.md) | `(int $microseconds): void` | `void` |
| [`preg_match()`](./builtins/regex/preg_match.md) | `(string $pattern, string $subject, array $matches): int` | `int` |
| [`preg_match_all()`](./builtins/regex/preg_match_all.md) | `(string $pattern, string $subject, array $matches): int` | `int` |
| [`preg_replace()`](./builtins/regex/preg_replace.md) | `(string $pattern, string $replacement, string $subject, int $limit = -1, int $count = null): string` | `string` |
| [`preg_replace_callback()`](./builtins/regex/preg_replace_callback.md) | `(string $pattern, callable $callback, string $subject, int $limit = -1, int $count = null, int $flags = 0): array` | `array` |
| [`preg_split()`](./builtins/regex/preg_split.md) | `(string $pattern, string $subject, int $limit, int $flags): array` | `array` |
| [`iterator_apply()`](./builtins/spl/iterator_apply.md) | `(traversable $iterator, callable $callback, array $args): int` | `int` |
| [`iterator_count()`](./builtins/spl/iterator_count.md) | `(traversable $iterator): int` | `int` |
| [`iterator_to_array()`](./builtins/spl/iterator_to_array.md) | `(traversable $iterator, bool $preserve_keys): array` | `array` |
| [`spl_autoload()`](./builtins/spl/spl_autoload.md) | `(string $class, string $file_extensions): void` | `void` |
| [`spl_autoload_call()`](./builtins/spl/spl_autoload_call.md) | `(string $class): void` | `void` |
| [`spl_autoload_extensions()`](./builtins/spl/spl_autoload_extensions.md) | `(string $file_extensions): string` | `string` |
| [`spl_autoload_functions()`](./builtins/spl/spl_autoload_functions.md) | `(): array` | `array` |
| [`spl_autoload_register()`](./builtins/spl/spl_autoload_register.md) | `(callable $callback, bool $throw, bool $prepend): bool` | `bool` |
| [`spl_autoload_unregister()`](./builtins/spl/spl_autoload_unregister.md) | `(callable $callback): bool` | `bool` |
| [`spl_classes()`](./builtins/spl/spl_classes.md) | `(): array` | `array` |
| [`spl_object_hash()`](./builtins/spl/spl_object_hash.md) | `(object $object): string` | `string` |
| [`spl_object_id()`](./builtins/spl/spl_object_id.md) | `(object $object): int` | `int` |
| [`fsockopen()`](./builtins/streams/fsockopen.md) | `(string $hostname, int $port, int $error_code, string $error_message, float $timeout): mixed` | `mixed` |
| [`pfsockopen()`](./builtins/streams/pfsockopen.md) | `(string $hostname, int $port, int $error_code, string $error_message, float $timeout): mixed` | `mixed` |
| [`stream_bucket_append()`](./builtins/streams/stream_bucket_append.md) | `(mixed $brigade, mixed $bucket): void` | `void` |
| [`stream_bucket_prepend()`](./builtins/streams/stream_bucket_prepend.md) | `(mixed $brigade, mixed $bucket): void` | `void` |
| [`stream_filter_append()`](./builtins/streams/stream_filter_append.md) | `(resource $stream, string $filter_name, int $mode, mixed $params): mixed` | `mixed` |
| [`stream_filter_prepend()`](./builtins/streams/stream_filter_prepend.md) | `(resource $stream, string $filter_name, int $mode, mixed $params): mixed` | `mixed` |
| [`addslashes()`](./builtins/string/addslashes.md) | `(string $string): string` | `string` |
| [`base64_decode()`](./builtins/string/base64_decode.md) | `(string $string, bool $strict): string` | `string` |
| [`base64_encode()`](./builtins/string/base64_encode.md) | `(string $string): string` | `string` |
| [`bin2hex()`](./builtins/string/bin2hex.md) | `(string $string): string` | `string` |
| [`chop()`](./builtins/string/chop.md) | `(string $string, string $characters): string` | `string` |
| [`chr()`](./builtins/string/chr.md) | `(int $codepoint): string` | `string` |
| [`crc32()`](./builtins/string/crc32.md) | `(string $string): int` | `int` |
| [`explode()`](./builtins/string/explode.md) | `(string $separator, string $string, int $limit): array` | `array` |
| [`grapheme_strrev()`](./builtins/string/grapheme_strrev.md) | `(string $string): mixed` | `mixed` |
| [`gzcompress()`](./builtins/string/gzcompress.md) | `(string $data, int $level, int $encoding): string` | `string` |
| [`gzdeflate()`](./builtins/string/gzdeflate.md) | `(string $data, int $level, int $encoding): string` | `string` |
| [`gzinflate()`](./builtins/string/gzinflate.md) | `(string $data, int $max_length): string` | `string` |
| [`gzuncompress()`](./builtins/string/gzuncompress.md) | `(string $data, int $max_length): string` | `string` |
| [`hash()`](./builtins/string/hash.md) | `(string $algo, string $data, bool $binary = false, array $options = []): string` | `string` |
| [`hash_algos()`](./builtins/string/hash_algos.md) | `(): array` | `array` |
| [`hash_copy()`](./builtins/string/hash_copy.md) | `(resource $context): mixed` | `mixed` |
| [`hash_equals()`](./builtins/string/hash_equals.md) | `(string $known_string, string $user_string): bool` | `bool` |
| [`hash_final()`](./builtins/string/hash_final.md) | `(resource $context, bool $binary): string` | `string` |
| [`hash_hmac()`](./builtins/string/hash_hmac.md) | `(string $algo, string $data, string $key, bool $binary): string` | `string` |
| [`hash_init()`](./builtins/string/hash_init.md) | `(string $algo, int $flags = 0, string $key = '', array $options = []): mixed` | `mixed` |
| [`hash_update()`](./builtins/string/hash_update.md) | `(resource $context, string $data): bool` | `bool` |
| [`hex2bin()`](./builtins/string/hex2bin.md) | `(string $string): string` | `string` |
| [`html_entity_decode()`](./builtins/string/html_entity_decode.md) | `(string $string, int $flags, string $encoding): string` | `string` |
| [`htmlentities()`](./builtins/string/htmlentities.md) | `(string $string, int $flags, string $encoding, bool $double_encode): string` | `string` |
| [`htmlspecialchars()`](./builtins/string/htmlspecialchars.md) | `(string $string, int $flags, string $encoding, bool $double_encode): string` | `string` |
| [`implode()`](./builtins/string/implode.md) | `(string $separator, array $array): string` | `string` |
| [`inet_ntop()`](./builtins/string/inet_ntop.md) | `(string $ip): mixed` | `mixed` |
| [`inet_pton()`](./builtins/string/inet_pton.md) | `(string $ip): mixed` | `mixed` |
| [`ip2long()`](./builtins/string/ip2long.md) | `(string $ip): mixed` | `mixed` |
| [`lcfirst()`](./builtins/string/lcfirst.md) | `(string $string): string` | `string` |
| [`long2ip()`](./builtins/string/long2ip.md) | `(int $ip): string` | `string` |
| [`ltrim()`](./builtins/string/ltrim.md) | `(string $string, string $characters): string` | `string` |
| [`md5()`](./builtins/string/md5.md) | `(string $string, bool $binary): string` | `string` |
| [`nl2br()`](./builtins/string/nl2br.md) | `(string $string, bool $use_xhtml): string` | `string` |
| [`number_format()`](./builtins/string/number_format.md) | `(float $num, int $decimals, string $decimal_separator, string $thousands_separator): string` | `string` |
| [`ord()`](./builtins/string/ord.md) | `(string $character): int` | `int` |
| [`printf()`](./builtins/string/printf.md) | `(string $format, ...$values): int` | `int` |
| [`rawurldecode()`](./builtins/string/rawurldecode.md) | `(string $string): string` | `string` |
| [`rawurlencode()`](./builtins/string/rawurlencode.md) | `(string $string): string` | `string` |
| [`rtrim()`](./builtins/string/rtrim.md) | `(string $string, string $characters): string` | `string` |
| [`sha1()`](./builtins/string/sha1.md) | `(string $string, bool $binary): string` | `string` |
| [`sprintf()`](./builtins/string/sprintf.md) | `(string $format, ...$values): string` | `string` |
| [`sscanf()`](./builtins/string/sscanf.md) | `(string $string, string $format, ...$vars): array` | `array` |
| [`str_contains()`](./builtins/string/str_contains.md) | `(string $haystack, string $needle): bool` | `bool` |
| [`str_ends_with()`](./builtins/string/str_ends_with.md) | `(string $haystack, string $needle): bool` | `bool` |
| [`str_ireplace()`](./builtins/string/str_ireplace.md) | `(mixed $search, mixed $replace, mixed $subject, int $count): mixed` | `mixed` |
| [`str_pad()`](./builtins/string/str_pad.md) | `(string $string, int $length, string $pad_string, int $pad_type): string` | `string` |
| [`str_repeat()`](./builtins/string/str_repeat.md) | `(string $string, int $times): string` | `string` |
| [`str_replace()`](./builtins/string/str_replace.md) | `(string $search, string $replace, string $subject, int $count): mixed` | `mixed` |
| [`str_split()`](./builtins/string/str_split.md) | `(string $string, int $length): array` | `array` |
| [`str_starts_with()`](./builtins/string/str_starts_with.md) | `(string $haystack, string $needle): bool` | `bool` |
| [`strcasecmp()`](./builtins/string/strcasecmp.md) | `(string $string1, string $string2): int` | `int` |
| [`strcmp()`](./builtins/string/strcmp.md) | `(string $string1, string $string2): int` | `int` |
| [`stripslashes()`](./builtins/string/stripslashes.md) | `(string $string): string` | `string` |
| [`strlen()`](./builtins/string/strlen.md) | `(string $string): int` | `int` |
| [`strpos()`](./builtins/string/strpos.md) | `(string $haystack, string $needle, int $offset): mixed` | `mixed` |
| [`strrev()`](./builtins/string/strrev.md) | `(string $string): string` | `string` |
| [`strrpos()`](./builtins/string/strrpos.md) | `(string $haystack, string $needle, int $offset): mixed` | `mixed` |
| [`strstr()`](./builtins/string/strstr.md) | `(string $haystack, string $needle, bool $before_needle): string` | `string` |
| [`strtolower()`](./builtins/string/strtolower.md) | `(string $string): string` | `string` |
| [`strtoupper()`](./builtins/string/strtoupper.md) | `(string $string): string` | `string` |
| [`substr()`](./builtins/string/substr.md) | `(string $string, int $offset, int $length): string` | `string` |
| [`substr_replace()`](./builtins/string/substr_replace.md) | `(string $string, string $replace, int $offset, int $length): string` | `string` |
| [`trim()`](./builtins/string/trim.md) | `(string $string, string $characters): string` | `string` |
| [`ucfirst()`](./builtins/string/ucfirst.md) | `(string $string): string` | `string` |
| [`ucwords()`](./builtins/string/ucwords.md) | `(string $string, string $separators): string` | `string` |
| [`urldecode()`](./builtins/string/urldecode.md) | `(string $string): string` | `string` |
| [`urlencode()`](./builtins/string/urlencode.md) | `(string $string): string` | `string` |
| [`vprintf()`](./builtins/string/vprintf.md) | `(string $format, array $values): int` | `int` |
| [`vsprintf()`](./builtins/string/vsprintf.md) | `(string $format, array $values): string` | `string` |
| [`wordwrap()`](./builtins/string/wordwrap.md) | `(string $string, int $width, string $break, bool $cut_long_words): string` | `string` |
| [`boolval()`](./builtins/type/boolval.md) | `(mixed $value): bool` | `bool` |
| [`ctype_alnum()`](./builtins/type/ctype_alnum.md) | `(string $text): bool` | `bool` |
| [`ctype_alpha()`](./builtins/type/ctype_alpha.md) | `(string $text): bool` | `bool` |
| [`ctype_digit()`](./builtins/type/ctype_digit.md) | `(string $text): bool` | `bool` |
| [`ctype_space()`](./builtins/type/ctype_space.md) | `(string $text): bool` | `bool` |
| [`floatval()`](./builtins/type/floatval.md) | `(mixed $value): float` | `float` |
| [`get_resource_id()`](./builtins/type/get_resource_id.md) | `(resource $resource): int` | `int` |
| [`get_resource_type()`](./builtins/type/get_resource_type.md) | `(resource $resource): string` | `string` |
| [`gettype()`](./builtins/type/gettype.md) | `(mixed $value): string` | `string` |
| [`intval()`](./builtins/type/intval.md) | `(mixed $value, int $base): int` | `int` |
| [`is_array()`](./builtins/type/is_array.md) | `(mixed $value): bool` | `bool` |
| [`is_bool()`](./builtins/type/is_bool.md) | `(mixed $value): bool` | `bool` |
| [`is_callable()`](./builtins/type/is_callable.md) | `(mixed $value, bool $syntax_only = false, string $callable_name = null): bool` | `bool` |
| [`is_float()`](./builtins/type/is_float.md) | `(mixed $value): bool` | `bool` |
| [`is_int()`](./builtins/type/is_int.md) | `(mixed $value): bool` | `bool` |
| [`is_iterable()`](./builtins/type/is_iterable.md) | `(mixed $value): bool` | `bool` |
| [`is_null()`](./builtins/type/is_null.md) | `(mixed $value): bool` | `bool` |
| [`is_numeric()`](./builtins/type/is_numeric.md) | `(mixed $value): bool` | `bool` |
| [`is_object()`](./builtins/type/is_object.md) | `(mixed $value): bool` | `bool` |
| [`is_resource()`](./builtins/type/is_resource.md) | `(mixed $value): bool` | `bool` |
| [`is_scalar()`](./builtins/type/is_scalar.md) | `(mixed $value): bool` | `bool` |
| [`is_string()`](./builtins/type/is_string.md) | `(mixed $value): bool` | `bool` |
| [`settype()`](./builtins/type/settype.md) | `(mixed $var, string $type): bool` | `bool` |
