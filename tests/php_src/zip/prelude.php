<?php
$__elephc_zip_open_basedir = '';
extern "elephc_zip" {
    function elephc_zip_open(string $path, int $path_len, int $flags): int;
}
extern "elephc_zip" {
    function elephc_zip_last_open_error(): int;
}
extern "elephc_zip" {
    function elephc_zip_used_empty_file(int $handle): int;
}
extern "elephc_zip" {
    function elephc_zip_close(int $handle): int;
}
extern "elephc_zip" {
    function elephc_zip_discard(int $handle): void;
}
extern "elephc_zip" {
    function elephc_zip_status(int $handle): int;
}
extern "elephc_zip" {
    function elephc_zip_status_sys(int $handle): int;
}
extern "elephc_zip" {
    function elephc_zip_status_string(int $handle): string;
}
extern "elephc_zip" {
    function elephc_zip_clear_error(int $handle): void;
}
extern "elephc_zip" {
    function elephc_zip_set_status(int $handle, int $code): void;
}
extern "elephc_zip" {
    function elephc_zip_result_ok(): int;
}
extern "elephc_zip" {
    function elephc_zip_last_id(int $handle): int;
}
extern "elephc_zip" {
    function elephc_zip_num_entries(int $handle): int;
}
extern "elephc_zip" {
    function elephc_zip_filename(int $handle): string;
}
extern "elephc_zip" {
    function elephc_zip_archive_comment(int $handle, int $flags): string;
}
extern "elephc_zip" {
    function elephc_zip_name(int $handle, int $index, int $raw, int $flags): string;
}
extern "elephc_zip" {
    function elephc_zip_locate(int $handle, string $name, int $name_len, int $flags): int;
}
extern "elephc_zip" {
    function elephc_zip_stat_index(int $handle, int $index, int $flags, int $field): int;
}
extern "elephc_zip" {
    function elephc_zip_stat_name(int $handle, string $name, int $name_len, int $flags, int $field): int;
}
extern "elephc_zip" {
    function elephc_zip_comment_index(int $handle, int $index, int $flags): string;
}
extern "elephc_zip" {
    function elephc_zip_comment_name(int $handle, string $name, int $name_len, int $flags): string;
}
extern "elephc_zip" {
    function elephc_zip_get_index(int $handle, int $index, int $len, int $flags): string;
}
extern "elephc_zip" {
    function elephc_zip_get_index_stream(int $handle, int $index, int $len, int $flags): string;
}
extern "elephc_zip" {
    function elephc_zip_stream_crc_error(): int;
}
extern "elephc_zip" {
    function elephc_zip_get_read_failed(): int;
}
extern "elephc_zip" {
    function elephc_zip_get_name(int $handle, string $name, int $name_len, int $len, int $flags): string;
}
extern "elephc_zip" {
    function elephc_zip_add_from_string(int $handle, string $name, int $name_len, string $data, int $data_len, int $flags): int;
}
extern "elephc_zip" {
    function elephc_zip_add_dir(int $handle, string $name, int $name_len, int $flags): int;
}
extern "elephc_zip" {
    function elephc_zip_add_file(int $handle, string $path, int $path_len, string $entry, int $entry_len, int $start, int $length, int $flags): int;
}
extern "elephc_zip" {
    function elephc_zip_replace_file(int $handle, string $path, int $path_len, int $index, int $start, int $length, int $flags): int;
}
extern "elephc_zip" {
    function elephc_zip_delete(int $handle, int $index): int;
}
extern "elephc_zip" {
    function elephc_zip_rename(int $handle, int $index, string $name, int $name_len): int;
}
extern "elephc_zip" {
    function elephc_zip_set_comment_index(int $handle, int $index, string $comment, int $comment_len): int;
}
extern "elephc_zip" {
    function elephc_zip_set_comment_name(int $handle, string $name, int $name_len, string $comment, int $comment_len): int;
}
extern "elephc_zip" {
    function elephc_zip_set_mtime_index(int $handle, int $index, int $timestamp, int $flags): int;
}
extern "elephc_zip" {
    function elephc_zip_set_mtime_name(int $handle, string $name, int $name_len, int $timestamp, int $flags): int;
}
extern "elephc_zip" {
    function elephc_zip_set_compression_index(int $handle, int $index, int $method, int $compflags): int;
}
extern "elephc_zip" {
    function elephc_zip_set_compression_name(int $handle, string $name, int $name_len, int $method, int $compflags): int;
}
extern "elephc_zip" {
    function elephc_zip_set_encryption_index(int $handle, int $index, int $method, string $password, int $password_len): int;
}
extern "elephc_zip" {
    function elephc_zip_set_encryption_name(int $handle, string $name, int $name_len, int $method, string $password, int $password_len): int;
}
extern "elephc_zip" {
    function elephc_zip_set_external_attributes_index(int $handle, int $index, int $opsys, int $attr, int $flags): int;
}
extern "elephc_zip" {
    function elephc_zip_set_external_attributes_name(int $handle, string $name, int $name_len, int $opsys, int $attr, int $flags): int;
}
extern "elephc_zip" {
    function elephc_zip_external_opsys(int $handle, int $index, int $flags): int;
}
extern "elephc_zip" {
    function elephc_zip_external_attr(int $handle, int $index, int $flags): int;
}
extern "elephc_zip" {
    function elephc_zip_external_opsys_name(int $handle, string $name, int $name_len, int $flags): int;
}
extern "elephc_zip" {
    function elephc_zip_external_attr_name(int $handle, string $name, int $name_len, int $flags): int;
}
extern "elephc_zip" {
    function elephc_zip_set_archive_comment(int $handle, string $comment, int $comment_len): int;
}
extern "elephc_zip" {
    function elephc_zip_set_password(int $handle, string $password, int $password_len): int;
}
extern "elephc_zip" {
    function elephc_zip_archive_flag(int $handle, int $flag, int $flags): int;
}
extern "elephc_zip" {
    function elephc_zip_set_archive_flag(int $handle, int $flag, int $value): int;
}
extern "elephc_zip" {
    function elephc_zip_unchange_all(int $handle): int;
}
extern "elephc_zip" {
    function elephc_zip_unchange_index(int $handle, int $index): int;
}
extern "elephc_zip" {
    function elephc_zip_unchange_name(int $handle, string $name, int $name_len): int;
}
extern "elephc_zip" {
    function elephc_zip_unchange_archive(int $handle): int;
}
extern "elephc_zip" {
    function elephc_zip_extract(int $handle, string $destination, int $destination_len, string $selection, int $selection_len): int;
}
extern "elephc_zip" {
    function elephc_zip_extract_failed_entry(int $handle): string;
}
extern "elephc_zip" {
    function elephc_zip_extract_warning(int $handle): string;
}
extern "elephc_zip" {
    function elephc_zip_set_progress_callback(int $handle, ptr $descriptor, ptr $adapter, float $rate): int;
}
extern "elephc_zip" {
    function elephc_zip_set_cancel_callback(int $handle, ptr $descriptor, ptr $adapter): int;
}
extern "elephc_zip" {
    function elephc_zip_compression_supported(int $method, int $encode): int;
}
extern "elephc_zip" {
    function elephc_zip_encryption_supported(int $method, int $encode): int;
}
extern "elephc_zip" {
    function elephc_zip_version(): string;
}
extern "elephc_zip" {
    function elephc_runtime_stream_wrapper_class(string $scheme): string;
}
extern "elephc_zip" {
    function elephc_zip_pattern_match(string $pattern, int $pattern_len, string $subject, int $subject_len): int;
}
class ZipArchive implements \Countable {
    const CREATE = 1;
    const EXCL = 2;
    const CHECKCONS = 4;
    const OVERWRITE = 8;
    const RDONLY = 16;
    const FL_NOCASE = 1;
    const FL_NODIR = 2;
    const FL_COMPRESSED = 4;
    const FL_UNCHANGED = 8;
    const FL_RECOMPRESS = 16;
    const FL_ENCRYPTED = 32;
    const FL_OVERWRITE = 8192;
    const FL_LOCAL = 256;
    const FL_CENTRAL = 512;
    const FL_ENC_GUESS = 0;
    const FL_ENC_RAW = 64;
    const FL_ENC_STRICT = 128;
    const FL_ENC_UTF_8 = 2048;
    const FL_ENC_CP437 = 4096;
    const FL_OPEN_FILE_NOW = 1073741824;
    const CM_DEFAULT = -1;
    const CM_STORE = 0;
    const CM_SHRINK = 1;
    const CM_REDUCE_1 = 2;
    const CM_REDUCE_2 = 3;
    const CM_REDUCE_3 = 4;
    const CM_REDUCE_4 = 5;
    const CM_IMPLODE = 6;
    const CM_DEFLATE = 8;
    const CM_DEFLATE64 = 9;
    const CM_PKWARE_IMPLODE = 10;
    const CM_BZIP2 = 12;
    const CM_LZMA = 14;
    const CM_LZMA2 = 33;
    const CM_ZSTD = 93;
    const CM_XZ = 95;
    const CM_TERSE = 18;
    const CM_LZ77 = 19;
    const CM_WAVPACK = 97;
    const CM_PPMD = 98;
    const ER_OK = 0;
    const ER_MULTIDISK = 1;
    const ER_RENAME = 2;
    const ER_CLOSE = 3;
    const ER_SEEK = 4;
    const ER_READ = 5;
    const ER_WRITE = 6;
    const ER_CRC = 7;
    const ER_ZIPCLOSED = 8;
    const ER_NOENT = 9;
    const ER_EXISTS = 10;
    const ER_OPEN = 11;
    const ER_TMPOPEN = 12;
    const ER_ZLIB = 13;
    const ER_MEMORY = 14;
    const ER_CHANGED = 15;
    const ER_COMPNOTSUPP = 16;
    const ER_EOF = 17;
    const ER_INVAL = 18;
    const ER_NOZIP = 19;
    const ER_INTERNAL = 20;
    const ER_INCONS = 21;
    const ER_REMOVE = 22;
    const ER_DELETED = 23;
    const ER_ENCRNOTSUPP = 24;
    const ER_RDONLY = 25;
    const ER_NOPASSWD = 26;
    const ER_WRONGPASSWD = 27;
    const ER_OPNOTSUPP = 28;
    const ER_INUSE = 29;
    const ER_TELL = 30;
    const ER_COMPRESSED_DATA = 31;
    const ER_CANCELLED = 32;
    const ER_DATA_LENGTH = 33;
    const ER_NOT_ALLOWED = 34;
    const ER_TRUNCATED_ZIP = 35;
    const AFL_RDONLY = 2;
    const AFL_IS_TORRENTZIP = 4;
    const AFL_WANT_TORRENTZIP = 8;
    const AFL_CREATE_OR_KEEP_FILE_FOR_EMPTY_ARCHIVE = 16;
    const OPSYS_DOS = 0;
    const OPSYS_AMIGA = 1;
    const OPSYS_OPENVMS = 2;
    const OPSYS_UNIX = 3;
    const OPSYS_VM_CMS = 4;
    const OPSYS_ATARI_ST = 5;
    const OPSYS_OS_2 = 6;
    const OPSYS_MACINTOSH = 7;
    const OPSYS_Z_SYSTEM = 8;
    const OPSYS_CPM = 9;
    const OPSYS_WINDOWS_NTFS = 10;
    const OPSYS_MVS = 11;
    const OPSYS_VSE = 12;
    const OPSYS_ACORN_RISC = 13;
    const OPSYS_VFAT = 14;
    const OPSYS_ALTERNATE_MVS = 15;
    const OPSYS_BEOS = 16;
    const OPSYS_TANDEM = 17;
    const OPSYS_OS_400 = 18;
    const OPSYS_OS_X = 19;
    const OPSYS_DEFAULT = 3;
    const EM_NONE = 0;
    const EM_TRAD_PKWARE = 1;
    const EM_AES_128 = 257;
    const EM_AES_192 = 258;
    const EM_AES_256 = 259;
    const EM_UNKNOWN = 65535;
    const LIBZIP_VERSION = '1.11.4';
    const LENGTH_TO_END = 0;
    const LENGTH_UNCHECKED = -2;
    private $__elephc_handle = 0;
    private $__elephc_status = 0;
    private $__elephc_status_sys = 0;
    private $__elephc_filename = '';
    private $__elephc_comment = '';
    private $__elephc_last_error = '';
    private $__elephc_progress_rate = 0.0;
    private mixed $__elephc_progress_callback = null;
    private mixed $__elephc_progress_bridge = null;
    private mixed $__elephc_cancel_callback = null;
    private mixed $__elephc_cancel_bridge = null;
    private $__elephc_has_progress = false;
    private $__elephc_has_cancel = false;
    private $__elephc_closing = false;
    private ?\Throwable $__elephc_pending_error = null;
    private int $__elephc_token = 0;
    public function __construct(mixed $filename = null) {
        if ($filename !== null) { }
        $this->__elephc_token = spl_object_id($this);
    }
    private function __elephc_require_open(): int {
        if ($this->__elephc_handle === 0) { throw new \ValueError('Invalid or uninitialized Zip object'); }
        return $this->__elephc_handle;
    }
    private function __elephc_require_mutable(): int {
        if ($this->__elephc_closing) {
            throw new \Error('Already being closed');
        }
        return $this->__elephc_require_open();
    }
    private function __elephc_refresh(): void {
        if ($this->__elephc_handle !== 0) {
            $handle = $this->__elephc_handle;
            $this->__elephc_status = elephc_zip_status($handle);
            $this->__elephc_status_sys = elephc_zip_status_sys($handle);
            $this->__elephc_last_error = $this->__elephc_status === self::ER_OK ? '' : elephc_zip_status_string($handle);
            $comment = elephc_zip_archive_comment($handle, 0);
            $this->__elephc_comment = elephc_zip_result_ok() === 1 ? hex2bin($comment) : '';
        }
    }
    private function __elephc_hex(string $encoded): string {
        $decoded = hex2bin($encoded);
        return $decoded === false ? '' : $decoded;
    }
    private function __elephc_name(int $handle, int $index, int $flags): string|false {
        $encoded = elephc_zip_name($handle, $index, ($flags & self::FL_ENC_RAW) !== 0 ? 1 : 0, $flags);
        if (elephc_zip_result_ok() !== 1) { return false; }
        return $this->__elephc_hex($encoded);
    }
    private function __elephc_stat(int $handle, int $index, int $flags): array|false {
        $field_flags = $flags;
        $probe = elephc_zip_stat_index($handle, $index, $field_flags, 0);
        if ($probe < 0) {
            $this->__elephc_refresh();
            return false;
        }
        $name = $this->__elephc_name($handle, $index, $flags);
        if ($name === false) {
            $this->__elephc_refresh();
            return false;
        }
        return ['name' => $name, 'index' => $probe, 'crc' => elephc_zip_stat_index($handle, $index, $field_flags, 1), 'size' => elephc_zip_stat_index($handle, $index, $field_flags, 2), 'mtime' => elephc_zip_stat_index($handle, $index, $field_flags, 3), 'comp_size' => elephc_zip_stat_index($handle, $index, $field_flags, 4), 'comp_method' => elephc_zip_stat_index($handle, $index, $field_flags, 5), 'encryption_method' => elephc_zip_stat_index($handle, $index, $field_flags, 6)];
    }
    private function __elephc_stat_for_name(int $handle, string $name, int $flags): array|false {
        $index = elephc_zip_locate($handle, $name, strlen($name), $flags);
        if ($index < 0) {
            $this->__elephc_refresh();
            return false;
        }
        return $this->__elephc_stat($handle, $index, $flags);
    }
    public function __get(string $name) {
        $handle = $this->__elephc_handle;
        if ($name === 'lastId') { return $handle === 0 ? -1 : elephc_zip_last_id($handle); }
        if ($name === 'status') { return $this->__elephc_status; }
        if ($name === 'statusSys') { return $this->__elephc_status_sys; }
        if ($name === 'numFiles') { return $handle === 0 ? 0 : elephc_zip_num_entries($handle); }
        if ($name === 'filename') { return $this->__elephc_filename; }
        if ($name === 'comment') { return $this->__elephc_comment; }
        __elephc_builtin_warning('Undefined property: ZipArchive::$' . $name);
        return null;
    }
    public function __isset(string $name): bool {
        return in_array($name, ['lastId', 'status', 'statusSys', 'numFiles', 'filename', 'comment'], true);
    }
    public function __set(string $name, $_value): void {
        throw new \Error('Cannot write read-only property ZipArchive::$' . $name);
    }
    public function __unset(string $name): void {
        throw new \Error('Cannot unset read-only property ZipArchive::$' . $name);
    }
    public function __debugInfo(): array {
        $info = ['lastId' => $this->__get('lastId'), 'status' => $this->__elephc_status, 'statusSys' => $this->__elephc_status_sys, 'numFiles' => $this->__get('numFiles'), 'filename' => $this->__elephc_filename, 'comment' => $this->__elephc_comment];
        $properties = (array) $this;
        foreach ($properties as $key => $value) {
            $key = (string) $key;
            $separator = strrpos($key, "\x00");
            if ($separator !== false) { $key = substr($key, $separator + 1); }
            if (str_starts_with($key, '__elephc_')) { continue 1; }
            if (!array_key_exists($key, $info)) { $info[$key] = $value; }
        }
        return $info;
    }
    public function __destruct() {
        if ($this->__elephc_handle !== 0) {
            $token = $this->__elephc_token;
            $this->__elephc_token = 0;
            $this->close();
            $this->__elephc_token = $token;
        }
    }
    public function open(string $filename, int $flags = 0): int|bool {
        if ($filename === '') { throw new \ValueError('ZipArchive::open(): Argument #1 ($filename) must not be empty'); }
        if (!$this->__elephc_check_open_basedir($filename, 'ZipArchive::open')) { return false; }
        if ($this->__elephc_handle !== 0 && !$this->close()) { return false; }
        $handle = elephc_zip_open($filename, strlen($filename), $flags);
        if ($handle === 0) {
            $this->__elephc_status = 0;
            $this->__elephc_status_sys = 0;
            $this->__elephc_last_error = '';
            $this->__elephc_filename = '';
            $this->__elephc_comment = '';
            return elephc_zip_last_open_error();
        }
        $this->__elephc_handle = $handle;
        $this->__elephc_closing = false;
        $this->__elephc_pending_error = null;
        if (elephc_zip_used_empty_file($handle) === 1) { __elephc_builtin_deprecated('ZipArchive::open(): Using empty file as ZipArchive is deprecated'); }
        $this->__elephc_filename = elephc_zip_filename($handle);
        $this->__elephc_status = 0;
        $this->__elephc_status_sys = 0;
        $this->__elephc_last_error = '';
        $comment = elephc_zip_archive_comment($handle, 0);
        $this->__elephc_comment = elephc_zip_result_ok() === 1 ? $this->__elephc_hex($comment) : '';
        return true;
    }
    public function setPassword(string $password): bool {
        $handle = $this->__elephc_require_mutable();
        if ($password === '') { return false; }
        $ok = elephc_zip_set_password($handle, $password, strlen($password));
        $this->__elephc_refresh();
        return $ok === 1;
    }
    public function close(): bool {
        if ($this->__elephc_closing) {
            throw new \Error('Already being closed');
        }
        $handle = $this->__elephc_require_open();
        $this->__elephc_closing = true;
        $this->__elephc_pending_error = null;
        __ElephcZipMemoryStream::markClosed($this->__elephc_token);
        $ok = elephc_zip_close($handle);
        $this->__elephc_status = elephc_zip_status($handle);
        $this->__elephc_status_sys = elephc_zip_status_sys($handle);
        $this->__elephc_last_error = $ok === 1 ? '' : elephc_zip_status_string($handle);
        // php-src warns with libzip's error string before returning, for a
        // cancellation too, and a callback exception is re-raised after the
        // method returns. Emit the warning first so both effects are visible in
        // the same order.
        if ($ok !== 1) {
            $reason = $this->__elephc_last_error;
            $system = (int) $this->__elephc_status_sys;
            if ($reason !== '' && $system === 2) { $reason = $reason . ': No such file or directory'; }
            if ($reason !== '' && $system === 13) { $reason = $reason . ': Permission denied'; }
            $this->__elephc_last_error = $reason;
            __elephc_builtin_warning('ZipArchive::close(): ' . $reason);
        }
        $error = $this->__elephc_pending_error;
        $this->__elephc_pending_error = null;
        elephc_zip_discard($handle);
        $this->__elephc_handle = 0;
        $this->__elephc_filename = '';
        $this->__elephc_comment = '';
        $this->__elephc_closing = false;
        if ($error !== null) { throw $error; }
        if ($ok !== 1) { return false; }
        return true;
    }
    public function count(): int {
        $handle = $this->__elephc_require_open();
        return elephc_zip_num_entries($handle);
    }
    public function getStatusString(): string {
        $handle = $this->__elephc_handle;
        if ($handle !== 0) { return elephc_zip_status_string($handle); }
        if ($this->__elephc_last_error !== '') { return $this->__elephc_last_error; }
        if ($this->__elephc_status === self::ER_CANCELLED) { return 'Operation cancelled'; }
        return 'No error';
    }
    public function clearError(): void {
        $this->__elephc_status = 0;
        $this->__elephc_status_sys = 0;
        $this->__elephc_last_error = '';
        if ($this->__elephc_handle !== 0) { elephc_zip_clear_error($this->__elephc_handle); }
    }
    public function addEmptyDir(string $dirname, int $flags = 0): bool {
        $handle = $this->__elephc_require_mutable();
        $index = elephc_zip_add_dir($handle, $dirname, strlen($dirname), $flags);
        $this->__elephc_refresh();
        return $index >= 0;
    }
    public function addFromString(string $name, string $content, int $flags = self::FL_OVERWRITE): bool {
        $handle = $this->__elephc_require_mutable();
        $index = elephc_zip_add_from_string($handle, $name, strlen($name), $content, strlen($content), $flags);
        $this->__elephc_refresh();
        return $index >= 0;
    }
    public function addFile(string $filepath, string $entryname = '', int $start = 0, int $length = self::LENGTH_TO_END, int $flags = self::FL_OVERWRITE): bool {
        if ($filepath === '') { throw new \ValueError('ZipArchive::addFile(): Argument #1 ($filepath) must not be empty'); }
        $handle = $this->__elephc_require_mutable();
        if (!$this->__elephc_check_open_basedir($filepath, 'ZipArchive::addFile')) { return false; }
        $index = elephc_zip_add_file($handle, $filepath, strlen($filepath), $entryname, strlen($entryname), $start, $length, $flags);
        if ($index === -2) {
            __elephc_builtin_warning('ZipArchive::addFile(): No such file or directory');
            return false;
        }
        if ($index === -3) {
            __elephc_builtin_warning('ZipArchive::addFile(): Invalid argument');
            $this->__elephc_refresh();
            return false;
        }
        $this->__elephc_refresh();
        return $index >= 0;
    }
    public function replaceFile(string $filepath, int $index, int $start = 0, int $length = self::LENGTH_TO_END, int $flags = 0): bool {
        if ($filepath === '') { throw new \ValueError('ZipArchive::replaceFile(): Argument #1 ($filepath) must not be empty'); }
        $handle = $this->__elephc_require_mutable();
        if (!$this->__elephc_check_open_basedir($filepath, 'ZipArchive::replaceFile')) { return false; }
        $result = elephc_zip_replace_file($handle, $filepath, strlen($filepath), $index, $start, $length, $flags);
        if ($result === -2) {
            __elephc_builtin_warning('ZipArchive::replaceFile(): No such file or directory');
            return false;
        }
        $this->__elephc_refresh();
        return $result >= 0;
    }
    public function getFromName(string $name, int $len = 0, int $flags = 0): string|false {
        $handle = $this->__elephc_require_open();
        if ($name === '') { throw new \ValueError('ZipArchive::getFromName(): Argument #1 ($name) must not be empty'); }
        $index = $this->locateName($name, $flags);
        if ($index === false) {
            $this->__elephc_refresh();
            return false;
        }
        $encoded = elephc_zip_get_index($handle, $index, $len, $flags);
        if (elephc_zip_result_ok() !== 1) {
            $this->__elephc_refresh();
            if (elephc_zip_get_read_failed() === 1) {
                __elephc_builtin_warning('ZipArchive::getFromName(): Cannot read entry: ' . $this->__elephc_last_error);
            }
            return false;
        }
        $this->__elephc_refresh();
        return $this->__elephc_hex($encoded);
    }
    public function getFromIndex(int $index, int $len = 0, int $flags = 0): string|false {
        $handle = $this->__elephc_require_open();
        if ($this->statIndex($index, $flags) === false) {
            $this->__elephc_refresh();
            return false;
        }
        $encoded = elephc_zip_get_index($handle, $index, $len, $flags);
        if (elephc_zip_result_ok() !== 1) {
            $this->__elephc_refresh();
            if (elephc_zip_get_read_failed() === 1) {
                __elephc_builtin_warning('ZipArchive::getFromIndex(): Cannot read entry: ' . $this->__elephc_last_error);
            }
            return false;
        }
        $this->__elephc_refresh();
        return $this->__elephc_hex($encoded);
    }
    public function getStream(string $name) {
        return $this->getStreamName($name);
    }
    public function getStreamName(string $name, int $flags = 0) {
        $handle = $this->__elephc_require_mutable();
        if ($name === '') { throw new \ValueError('ZipArchive::getStreamName(): Argument #1 ($name) must not be empty'); }
        $index = elephc_zip_locate($handle, $name, strlen($name), $flags);
        if ($index < 0) {
            $this->__elephc_refresh();
            return false;
        }
        return $this->getStreamIndex($index, $flags);
    }
    public function getStreamIndex(int $index, int $flags = 0) {
        $handle = $this->__elephc_require_mutable();
        $encoded = elephc_zip_get_index_stream($handle, $index, 0, $flags);
        if (elephc_zip_result_ok() !== 1) {
            $this->__elephc_refresh();
            return false;
        }
        $crc_error = elephc_zip_stream_crc_error() === 1;
        $data = $this->__elephc_hex($encoded);
        $stat = $this->statIndex($index, $flags);
        if ($stat === false) { return false; }
        $seekable = $stat['comp_method'] === ZipArchive::CM_STORE;
        $token = $this->__elephc_token;
        $stream_id = __ElephcZipMemoryStream::$next_stream_id;
        __ElephcZipMemoryStream::$next_stream_id = __ElephcZipMemoryStream::$next_stream_id + 1;
        $stream_token = $token . '_' . $stream_id;
        $key = $stream_token . ':' . $stat['name'];
        __ElephcZipMemoryStream::$streams[$key] = ['data' => $data, 'stat' => $stat, 'seekable' => $seekable, 'crc_error' => $crc_error, 'closed' => false, 'closed_warned' => false, 'progress_callback' => $this->__elephc_progress_callback, 'cancel_callback' => $this->__elephc_cancel_callback];
        $stream = fopen('elephc-zip-memory://' . $stream_token . '/' . $stat['name'], 'rb');
        if ($stream === false) {
            unset(__ElephcZipMemoryStream::$streams[$key]);
            return false;
        }
        return $stream;
    }
    public function getNameIndex(int $index, int $flags = 0): string|false {
        $handle = $this->__elephc_require_open();
        return $this->__elephc_name($handle, $index, $flags);
    }
    public function locateName(string $name, int $flags = 0): int|false {
        $handle = $this->__elephc_require_open();
        $index = elephc_zip_locate($handle, $name, strlen($name), $flags);
        $this->__elephc_refresh();
        return $index >= 0 ? $index : false;
    }
    public function statIndex(int $index, int $flags = 0): array|false {
        $handle = $this->__elephc_require_open();
        return $this->__elephc_stat($handle, $index, $flags);
    }
    public function statName(string $name, int $flags = 0): array|false {
        $handle = $this->__elephc_require_open();
        if ($name === '') { throw new \ValueError('ZipArchive::statName(): Argument #1 ($name) must not be empty'); }
        return $this->__elephc_stat_for_name($handle, $name, $flags);
    }
    public function deleteIndex(int $index): bool {
        $handle = $this->__elephc_require_mutable();
        $ok = elephc_zip_delete($handle, $index);
        $this->__elephc_refresh();
        return $ok === 1;
    }
    public function deleteName(string $name): bool {
        $handle = $this->__elephc_require_mutable();
        $index = elephc_zip_locate($handle, $name, strlen($name), 0);
        if ($index < 0) {
            $this->__elephc_refresh();
            return false;
        }
        return $this->deleteIndex($index);
    }
    public function renameIndex(int $index, string $new_name): bool {
        $handle = $this->__elephc_require_mutable();
        if ($new_name === '') { throw new \ValueError('ZipArchive::renameIndex(): Argument #2 ($new_name) must not be empty'); }
        $ok = elephc_zip_rename($handle, $index, $new_name, strlen($new_name));
        $this->__elephc_refresh();
        return $ok === 1;
    }
    public function renameName(string $name, string $new_name): bool {
        $handle = $this->__elephc_require_mutable();
        if ($new_name === '') { throw new \ValueError('ZipArchive::renameName(): Argument #2 ($new_name) must not be empty'); }
        $index = elephc_zip_locate($handle, $name, strlen($name), 0);
        if ($index < 0) {
            $this->__elephc_refresh();
            return false;
        }
        return $this->renameIndex($index, $new_name);
    }
    public function setArchiveComment(string $comment): bool {
        if (strlen($comment) > 65535) { throw new \ValueError('ZipArchive::setArchiveComment(): Argument #1 ($comment) must be less than 65535 bytes'); }
        $handle = $this->__elephc_require_mutable();
        $ok = elephc_zip_set_archive_comment($handle, $comment, strlen($comment));
        $this->__elephc_refresh();
        return $ok === 1;
    }
    public function getArchiveComment(int $flags = 0): string|false {
        $handle = $this->__elephc_require_open();
        $encoded = elephc_zip_archive_comment($handle, $flags);
        if (elephc_zip_result_ok() !== 1) { return false; }
        return $this->__elephc_hex($encoded);
    }
    public function setArchiveFlag(int $flag, int $value): bool {
        $handle = $this->__elephc_require_mutable();
        $ok = elephc_zip_set_archive_flag($handle, $flag, $value);
        $this->__elephc_refresh();
        return $ok === 1;
    }
    public function getArchiveFlag(int $flag, int $flags = 0): int {
        $handle = $this->__elephc_require_open();
        return elephc_zip_archive_flag($handle, $flag, $flags);
    }
    public function setCommentIndex(int $index, string $comment): bool {
        if (strlen($comment) > 65535) { throw new \ValueError('ZipArchive::setCommentIndex(): Argument #2 ($comment) must be less than 65535 bytes'); }
        $handle = $this->__elephc_require_mutable();
        $ok = elephc_zip_set_comment_index($handle, $index, $comment, strlen($comment));
        $this->__elephc_refresh();
        return $ok === 1;
    }
    public function setCommentName(string $name, string $comment): bool {
        if (strlen($comment) > 65535) { throw new \ValueError('ZipArchive::setCommentName(): Argument #2 ($comment) must be less than 65535 bytes'); }
        $handle = $this->__elephc_require_mutable();
        if ($name === '') { throw new \ValueError('ZipArchive::setCommentName(): Argument #1 ($name) must not be empty'); }
        $ok = elephc_zip_set_comment_name($handle, $name, strlen($name), $comment, strlen($comment));
        $this->__elephc_refresh();
        return $ok === 1;
    }
    public function getCommentIndex(int $index, int $flags = 0): string|false {
        $handle = $this->__elephc_require_open();
        $encoded = elephc_zip_comment_index($handle, $index, $flags);
        if (elephc_zip_result_ok() !== 1) { return false; }
        return $this->__elephc_hex($encoded);
    }
    public function getCommentName(string $name, int $flags = 0): string|false {
        if ($name === '') { throw new \ValueError('ZipArchive::getCommentName(): Argument #1 ($name) must not be empty'); }
        $handle = $this->__elephc_require_open();
        $encoded = elephc_zip_comment_name($handle, $name, strlen($name), $flags);
        if (elephc_zip_result_ok() !== 1) { return false; }
        return $this->__elephc_hex($encoded);
    }
    public function setMtimeIndex(int $index, int $timestamp, int $flags = 0): bool {
        $handle = $this->__elephc_require_mutable();
        $ok = elephc_zip_set_mtime_index($handle, $index, $timestamp, $flags);
        $this->__elephc_refresh();
        return $ok === 1;
    }
    public function setMtimeName(string $name, int $timestamp, int $flags = 0): bool {
        $handle = $this->__elephc_require_mutable();
        if ($name === '') { throw new \ValueError('ZipArchive::setMtimeName(): Argument #1 ($name) must not be empty'); }
        $ok = elephc_zip_set_mtime_name($handle, $name, strlen($name), $timestamp, $flags);
        $this->__elephc_refresh();
        return $ok === 1;
    }
    public function setCompressionIndex(int $index, int $method, int $compflags = 0): bool {
        $handle = $this->__elephc_require_mutable();
        $ok = elephc_zip_set_compression_index($handle, $index, $method, $compflags);
        $this->__elephc_refresh();
        return $ok === 1;
    }
    public function setCompressionName(string $name, int $method, int $compflags = 0): bool {
        $handle = $this->__elephc_require_mutable();
        if ($name === '') { throw new \ValueError('ZipArchive::setCompressionName(): Argument #1 ($name) must not be empty'); }
        $ok = elephc_zip_set_compression_name($handle, $name, strlen($name), $method, $compflags);
        $this->__elephc_refresh();
        return $ok === 1;
    }
    public function setEncryptionIndex(int $index, int $method, ?string $password = null): bool {
        $handle = $this->__elephc_require_mutable();
        $secret = $password === null ? '' : $password;
        $length = $password === null ? -1 : strlen($secret);
        $ok = elephc_zip_set_encryption_index($handle, $index, $method, $secret, $length);
        $this->__elephc_refresh();
        return $ok === 1;
    }
    public function setEncryptionName(string $name, int $method, ?string $password = null): bool {
        $handle = $this->__elephc_require_mutable();
        if ($name === '') { throw new \ValueError('ZipArchive::setEncryptionName(): Argument #1 ($name) must not be empty'); }
        $secret = $password === null ? '' : $password;
        $length = $password === null ? -1 : strlen($secret);
        $ok = elephc_zip_set_encryption_name($handle, $name, strlen($name), $method, $secret, $length);
        $this->__elephc_refresh();
        return $ok === 1;
    }
    public function setExternalAttributesIndex(int $index, int $opsys, int $attr, int $flags = 0): bool {
        $handle = $this->__elephc_require_mutable();
        $ok = elephc_zip_set_external_attributes_index($handle, $index, $opsys, $attr, $flags);
        $this->__elephc_refresh();
        return $ok === 1;
    }
    public function setExternalAttributesName(string $name, int $opsys, int $attr, int $flags = 0): bool {
        $handle = $this->__elephc_require_mutable();
        if ($name === '') { throw new \ValueError('ZipArchive::setExternalAttributesName(): Argument #1 ($name) must not be empty'); }
        $ok = elephc_zip_set_external_attributes_name($handle, $name, strlen($name), $opsys, $attr, $flags);
        $this->__elephc_refresh();
        return $ok === 1;
    }
    public function getExternalAttributesIndex(int $index, &$opsys, &$attr, int $flags = 0): bool {
        $handle = $this->__elephc_require_open();
        $opsys = elephc_zip_external_opsys($handle, $index, $flags);
        if ($opsys < 0) {
            $this->__elephc_refresh();
            return false;
        }
        $attr = elephc_zip_external_attr($handle, $index, $flags);
        $this->__elephc_refresh();
        return true;
    }
    public function getExternalAttributesName(string $name, &$opsys, &$attr, int $flags = 0): bool {
        if ($name === '') { throw new \ValueError('ZipArchive::getExternalAttributesName(): Argument #1 ($name) must not be empty'); }
        $handle = $this->__elephc_require_open();
        $opsys = elephc_zip_external_opsys_name($handle, $name, strlen($name), $flags);
        if ($opsys < 0) {
            $this->__elephc_refresh();
            return false;
        }
        $attr = elephc_zip_external_attr_name($handle, $name, strlen($name), $flags);
        $this->__elephc_refresh();
        return true;
    }
    public function unchangeIndex(int $index): bool {
        $handle = $this->__elephc_require_mutable();
        $ok = elephc_zip_unchange_index($handle, $index);
        $this->__elephc_refresh();
        return $ok === 1;
    }
    public function unchangeName(string $name): bool {
        $handle = $this->__elephc_require_mutable();
        $ok = elephc_zip_unchange_name($handle, $name, strlen($name));
        $this->__elephc_refresh();
        return $ok === 1;
    }
    public function unchangeAll(): bool {
        $handle = $this->__elephc_require_mutable();
        $ok = elephc_zip_unchange_all($handle);
        $this->__elephc_refresh();
        return $ok === 1;
    }
    public function unchangeArchive(): bool {
        $handle = $this->__elephc_require_mutable();
        $ok = elephc_zip_unchange_archive($handle);
        $this->__elephc_refresh();
        return $ok === 1;
    }
    public function extractTo(string $pathto, array|string|null $files = null): bool {
        $handle = $this->__elephc_require_open();
        if (!$this->__elephc_check_open_basedir($pathto, 'ZipArchive::extractTo')) { return false; }
        $scheme_separator = strpos($pathto, '://');
        if ($scheme_separator !== false) {
            $scheme = strtolower(substr($pathto, 0, $scheme_separator));
            if ($scheme === 'file') {
                $local = substr($pathto, 7);
                if (str_starts_with($local, 'localhost')) {
                    $local = substr($local, 9);
                    if ($local === '') { $local = '/'; }
                } elseif ($local !== '' && $local[0] !== '/') {
                    return false;
                }
                $pathto = $local;
            } else {
                $class = elephc_runtime_stream_wrapper_class($scheme);
                $first = $this->getNameIndex(0);
                if ($class !== '' && $first !== false) { __elephc_builtin_warning('ZipArchive::extractTo(' . rtrim($pathto, '/') . '/' . $first . '): Failed to open stream: "' . $class . '::stream_open" is not implemented'); }
                return false;
            }
        } elseif (str_starts_with($pathto, 'file:/')) {
            $pathto = substr($pathto, 5);
        }
        $selection = 'null';
        $ok = true;
        if ($files !== null) {
            $list = is_array($files) ? $files : [$files];
            $names = [];
            $count = count($list);
            if ($count === 0) {
                $ok = false;
            } else {
                foreach ($list as $name) {
                    if (!is_string($name)) { continue 1; }
                    if ($this->locateName($name) === false) {
                        $ok = false;
                        break 1;
                    }
                    $names[] = $name;
                }
            }
            $encoded = json_encode($names);
            if ($encoded !== false) {
                $selection = $encoded;
            } else {
                $selection = '[]';
                $ok = false;
            }
        }
        $extracted = elephc_zip_extract($handle, $pathto, strlen($pathto), $selection, strlen($selection));
        $this->__elephc_refresh();
        if ($extracted !== 1) {
            $warning = elephc_zip_extract_warning($handle);
            if ($warning !== '') {
                __elephc_builtin_warning($warning);
            } else {
                $failed = $this->__elephc_hex(elephc_zip_extract_failed_entry($handle));
                if ($failed !== '' && $this->__elephc_last_error !== '') {
                    __elephc_builtin_warning('ZipArchive::extractTo(): Cannot extract "' . $failed . '": "' . $this->__elephc_last_error . '"');
                }
            }
        }
        return $ok && $extracted === 1;
    }
    private function __elephc_brace_patterns(string $pattern): array {
        $open = strpos($pattern, '{');
        if ($open === false) { return [$pattern]; }
        $close = strpos($pattern, '}', $open);
        if ($close === false) { return [$pattern]; }
        $prefix = substr($pattern, 0, $open);
        $suffix = substr($pattern, $close + 1);
        $parts = explode(',', substr($pattern, $open + 1, $close - $open - 1));
        $result = [];
        foreach ($parts as $part) { foreach ($this->__elephc_brace_patterns($prefix . $part . $suffix) as $expanded) { $result[] = $expanded; } }
        return $result;
    }
    private function __elephc_entry_from(string $file, mixed $options, string $function): string|false {
        $options = (array) $options;
        $stripped = $file;
        if (!empty($options['remove_all_path'])) {
            $stripped = basename($file);
        } elseif (isset($options['remove_path'])) {
            $remove = (string) $options['remove_path'];
            if ($remove !== '' && strlen($file) > strlen($remove) && str_starts_with($file, $remove)) {
                if (substr($file, strlen($remove), 1) === '/') {
                    $stripped = substr($file, strlen($remove) + 1);
                } else {
                    $stripped = substr($file, strlen($remove));
                }
            }
        }
        if (isset($options['add_path']) && (string) $options['add_path'] !== '') {
            $add = (string) $options['add_path'];
            $total = strlen($add) + strlen($stripped);
            if ($total > PHP_MAXPATHLEN - 1) {
                __elephc_builtin_warning($function . '(): Entry name too long (max: ' . PHP_MAXPATHLEN - 1 . ', ' . $total . ' given)');
                return false;
            }
            $stripped = $add . $stripped;
        }
        return $stripped;
    }
    private function __elephc_check_open_basedir(string $path, string $function): bool {
        global $__elephc_zip_open_basedir;
        if ($__elephc_zip_open_basedir === '') { return true; }
        $allowed = (string) $__elephc_zip_open_basedir;
        // php-src resolves the path before comparing it with open_basedir: existing
        // components go through realpath(), so `..` cannot step across a symlink
        // that points outside the allowed roots. A missing final component is
        // appended to its resolved parent.
        $original = $path;
        $resolved = realpath($path);
        if ($resolved === false) {
            $tail = '';
            $candidate = $path;
            while (true) {
                $parent = dirname($candidate);
                if ($parent === $candidate) { break; }
                $tail = '/' . basename($candidate) . $tail;
                $candidate = $parent;
                $resolved = realpath($candidate);
                if ($resolved !== false) {
                    $resolved = rtrim((string) $resolved, '/') . $tail;
                    break;
                }
            }
            if ($resolved === false) { $resolved = $path; }
        }
        if (!str_starts_with((string) $resolved, '/')) {
            $resolved = (string) getcwd() . '/' . (string) $resolved;
        }
        $segments = [];
        foreach (explode('/', (string) $resolved) as $component) {
            if ($component === '' || $component === '.') { continue; }
            if ($component === '..') {
                if (count($segments) > 0) { array_pop($segments); }
                continue;
            }
            $segments[] = $component;
        }
        $path = '/' . implode('/', $segments);
        foreach (explode(':', $allowed) as $root) {
            if ($root === '/') { return true; }
            $real_root = realpath($root);
            if ($real_root !== false) {
                $root = (string) $real_root;
            } else {
                if (!str_starts_with($root, '/')) {
                    $root = (string) getcwd() . '/' . $root;
                }
            }
            $root = rtrim($root, '/');
            if ($root !== '' && ($path === $root || str_starts_with($path, $root . '/'))) { return true; }
        }
        __elephc_builtin_warning($function . '(): open_basedir restriction in effect. File(' . $original . ') is not within the allowed path(s): (' . $allowed . ')');
        return false;
    }
    private function __elephc_add_matches(array $files, mixed $options, string $function): array|false {
        $options = (array) $options;
        $added = [];
        foreach ($files as $file) {
            $entry = $this->__elephc_entry_from($file, $options, $function);
            if ($entry === false) { return false; }
            $flags = isset($options['flags']) ? (int) $options['flags'] : self::FL_OVERWRITE;
            if (!$this->addFile($file, $entry, 0, self::LENGTH_TO_END, $flags)) { return false; }
            if (isset($options['comp_method']) && !$this->setCompressionName($entry, (int) $options['comp_method'], isset($options['comp_flags']) ? (int) $options['comp_flags'] : 0)) { return false; }
            if (isset($options['enc_method']) && !$this->setEncryptionName($entry, (int) $options['enc_method'], isset($options['enc_password']) ? (string) $options['enc_password'] : null)) { return false; }
            $added[] = $file;
        }
        return $added;
    }
    public function addGlob(string $pattern, int $flags = 0, mixed $options = []): array|false {
        $this->__elephc_require_mutable();
        $brace_expansion = ($flags & GLOB_BRACE) !== 0;
        if (!$this->__elephc_check_open_basedir($pattern, 'ZipArchive::addGlob')) { return false; }
        if ($pattern === '') { throw new \ValueError('ZipArchive::addGlob(): Argument #1 ($pattern) must not be empty'); }
        $matches = [];
        $patterns = $brace_expansion ? $this->__elephc_brace_patterns($pattern) : [$pattern];
        foreach ($patterns as $expanded) {
            $found = glob($expanded);
            if ($found === false || count($found) === 0) {
                if (is_file($expanded)) {
                    $found = [$expanded];
                } else {
                    if ($found === false) { return false; }
                    continue 1;
                }
            }
            foreach ($found as $file) { $matches[] = $file; }
        }
        if (count($matches) === 0) { return []; }
        return $this->__elephc_add_matches($matches, $options, 'ZipArchive::addGlob');
    }
    private function __elephc_pattern_has_ending_delimiter(string $pattern): bool {
        $delimiter = substr($pattern, 0, 1);
        if ($delimiter === '') { return false; }
        $escaped = false;
        $length = strlen($pattern);
        for ($i = 1; $i < $length; $i = $i + 1) {
            $char = $pattern[$i];
            if ($escaped) { $escaped = false; continue; }
            if ($char === '\\') { $escaped = true; continue; }
            if ($char === $delimiter) { return true; }
        }
        return false;
    }
    public function addPattern(string $pattern, string $path = '.', mixed $options = []): array|false {
        $this->__elephc_require_mutable();
        if (!$this->__elephc_check_open_basedir($path, 'ZipArchive::addPattern')) { return false; }
        if ($pattern === '') { throw new \ValueError('ZipArchive::addPattern(): Argument #1 ($pattern) must not be empty'); }
        $directory = opendir($path);
        if ($directory === false) { return false; }
        $names = [];
        while (($name = readdir($directory)) !== false) {
            $name = (string) $name;
            if ($name === '.' || $name === '..') { continue 1; }
            $full = rtrim($path, '/') . '/' . $name;
            if (is_dir($full)) { continue 1; }
            $names[] = $name;
        }
        closedir($directory);
        sort($names);
        if (count($names) === 0) { return []; }
        if (!$this->__elephc_pattern_has_ending_delimiter($pattern)) {
            __elephc_builtin_warning('ZipArchive::addPattern(): No ending delimiter \'' . substr($pattern, 0, 1) . '\' found');
            __elephc_builtin_warning('ZipArchive::addPattern(): Invalid expression');
            return false;
        }
        $matches = [];
        foreach ($names as $name) {
            $matched = elephc_zip_pattern_match($pattern, strlen($pattern), (string) $name, strlen((string) $name));
            if ($matched < 0) {
                __elephc_builtin_warning('ZipArchive::addPattern(): Invalid expression');
                return false;
            }
            if ($matched === 1) { $matches[] = rtrim($path, '/') . '/' . $name; }
        }
        if (count($matches) === 0) { return []; }
        return $this->__elephc_add_matches($matches, $options, 'ZipArchive::addPattern');
    }
    public function registerProgressCallback(float $rate, callable $callback): bool {
        $handle = $this->__elephc_require_mutable();
        if ($rate < 0.0 || $rate > 1.0) { return false; }
        $bridge = function ($_ignored, $progress): void {
            if ($this->__elephc_pending_error !== null) { return; }
            try {
                ($this->__elephc_progress_callback)($progress);
            } catch (\Throwable $_error) {
                $this->__elephc_pending_error = $_error;
            }
        };
        $descriptor = __elephc_callable_ptr(__elephc_normalize_callable($bridge));
        $adapter = __elephc_curl_adapter_addr();
        if (elephc_zip_set_progress_callback($handle, $descriptor, $adapter, $rate) !== 1) { return false; }
        $this->__elephc_progress_rate = $rate;
        $this->__elephc_progress_callback = $callback;
        $this->__elephc_progress_bridge = $bridge;
        $this->__elephc_has_progress = true;
        return true;
    }
    public function registerCancelCallback(callable $callback): bool {
        $handle = $this->__elephc_require_mutable();
        $bridge = function ($_ignored): int {
            if ($this->__elephc_pending_error !== null) { return 1; }
            try {
                $returned = ($this->__elephc_cancel_callback)();
                if (is_bool($returned)) { return $returned ? 1 : 0; }
                if (!is_int($returned)) {
                    $this->__elephc_pending_error = new \TypeError('Return value of callback provided to ZipArchive::registerCancelCallback() must be of type int, ' . gettype($returned) . ' returned');
                    return 1;
                }
                return $returned;
            } catch (\Throwable $_error) {
                $this->__elephc_pending_error = $_error;
                return 1;
            }
        };
        $descriptor = __elephc_callable_ptr(__elephc_normalize_callable($bridge));
        $adapter = __elephc_curl_adapter_addr();
        if (elephc_zip_set_cancel_callback($handle, $descriptor, $adapter) !== 1) { return false; }
        $this->__elephc_cancel_callback = $callback;
        $this->__elephc_cancel_bridge = $bridge;
        $this->__elephc_has_cancel = true;
        return true;
    }
    public static function isCompressionMethodSupported(int $method, bool $enc = true): bool {
        return elephc_zip_compression_supported($method, $enc ? 1 : 0) === 1;
    }
    public static function isEncryptionMethodSupported(int $method, bool $enc = true): bool {
        return elephc_zip_encryption_supported($method, $enc ? 1 : 0) === 1;
    }
}
final class __ElephcZipProcedural {
    public static array $state = [];
    public static function closeResource(mixed $resource): void {
        if (is_resource($resource)) { fclose($resource); }
    }
}
final class __ElephcZipMemoryStream {
    public $context = null;
    public static array $streams = [];
    public static int $next_stream_id = 0;
    private string $__elephc_key = '';
    public static function markClosed(int $token): void {
        if (!isset(self::$streams)) { return; }
        foreach (array_keys(self::$streams) as $key) {
            if (str_starts_with($key, $token . ':') || str_starts_with($key, $token . '_')) {
                self::$streams[$key]['closed'] = true;
                self::$streams[$key]['data'] = '';
            }
        }
    }
    public function stream_open(string $path, string $_mode, int $_options, &$opened_path): bool {
        $rest = substr($path, strlen('elephc-zip-memory://'));
        $separator = strpos($rest, '/');
        if ($separator === false) { return false; }
        $token = substr($rest, 0, $separator);
        $entry = substr($rest, $separator + 1);
        $key = $token . ':' . $entry;
        if (!isset(self::$streams[$key])) { return false; }
        $this->__elephc_key = $key;
        return true;
    }
    public function stream_read(int $count): string|false {
        if (!isset(self::$streams[$this->__elephc_key])) { return false; }
        if (self::$streams[$this->__elephc_key]['closed']) {
            __elephc_builtin_warning('stream_get_contents(): Zip stream error: Containing zip archive was closed');
            return '';
        }
        if (self::$streams[$this->__elephc_key]['crc_error']) {
            self::$streams[$this->__elephc_key]['crc_error'] = false;
            __elephc_builtin_warning('fread(): Zip stream error: CRC error');
        }
        $data = self::$streams[$this->__elephc_key]['data'];
        $position = self::$streams[$this->__elephc_key]['position'] ?? 0;
        $chunk = substr($data, $position, $count);
        self::$streams[$this->__elephc_key]['position'] = $position + strlen($chunk);
        return $chunk;
    }
    public function stream_eof(): bool {
        if (!isset(self::$streams[$this->__elephc_key])) { return true; }
        if (self::$streams[$this->__elephc_key]['closed']) {
            if (!self::$streams[$this->__elephc_key]['closed_warned']) {
                self::$streams[$this->__elephc_key]['closed_warned'] = true;
                __elephc_builtin_warning('stream_get_contents(): Zip stream error: Containing zip archive was closed');
            }
            return true;
        }
        $data = self::$streams[$this->__elephc_key]['data'];
        $position = self::$streams[$this->__elephc_key]['position'] ?? 0;
        return $position >= strlen($data);
    }
    public function stream_tell(): int {
        return self::$streams[$this->__elephc_key]['position'] ?? 0;
    }
    public function stream_seek(int $offset, int $whence = 0): bool {
        if (!isset(self::$streams[$this->__elephc_key])) { return false; }
        $state = self::$streams[$this->__elephc_key];
        if ($state['closed'] || !$state['seekable']) {
            __elephc_builtin_warning('fseek(): Zip stream error: does not support seeking');
            return false;
        }
        $position = $state['position'] ?? 0;
        if ($whence === 1) {
            $offset = $offset + $position;
        } elseif ($whence === 2) {
            $offset = $offset + strlen($state['data']);
        }
        if ($offset < 0) { return false; }
        self::$streams[$this->__elephc_key]['position'] = $offset;
        return true;
    }
    public function stream_stat(): array|false {
        $state = self::$streams[$this->__elephc_key] ?? [];
        if (!isset($state['stat'])) { return false; }
        $stat = (array) $state['stat'];
        $size = (int) ($stat['size'] ?? 0);
        $mtime = (int) ($stat['mtime'] ?? time());
        $mode = 32768;
        return [0 => 0, 1 => -1, 2 => $mode, 3 => 1, 4 => 0, 5 => 0, 6 => -1, 7 => $size, 8 => $mtime, 9 => $mtime, 10 => $mtime, 11 => -1, 12 => -1, 'dev' => 0, 'ino' => -1, 'mode' => $mode, 'nlink' => 1, 'uid' => 0, 'gid' => 0, 'rdev' => -1, 'size' => $size, 'atime' => $mtime, 'mtime' => $mtime, 'ctime' => $mtime, 'blksize' => -1, 'blocks' => -1];
    }
    public function stream_close(): void {
        unset(self::$streams[$this->__elephc_key]);
    }
    public function url_stat(string $_path, int $_flags): array|false {
        return false;
    }
}
stream_wrapper_register('elephc-zip-memory', '__ElephcZipMemoryStream');
final class __ElephcZipStreamWrapper {
    public $context = null;
    private string $__elephc_data = '';
    private int $__elephc_position = 0;
    private array $__elephc_stat = [];
    private bool $__elephc_seekable = true;
    private string $__elephc_archive_name = '';
    private string $__elephc_entry = '';
    public function stream_open(string $path, string $_mode, int $_options, &$opened_path): bool {
        if ($_mode === '' || $_mode[0] !== 'r') { return false; }
        $separator = strpos($path, '#');
        if ($separator === false || strtolower(substr($path, 0, 6)) !== 'zip://') { return false; }
        $archive_name = substr($path, 6, $separator - 6);
        if ($archive_name === '') { return false; }
        $entry = substr($path, $separator + 1);
        $this->__elephc_archive_name = $archive_name;
        $this->__elephc_entry = $entry;
        $archive = new ZipArchive();
        $opened = $archive->open($archive_name, ZipArchive::RDONLY);
        if ($opened !== true) { return false; }
        $context = stream_context_get_default();
        $context_options = stream_context_get_options($context);
        if (isset($context_options['zip']['password'])) { $archive->setPassword((string) $context_options['zip']['password']); }
        $stream = $archive->getStream($entry);
        if (!is_resource($stream)) { $archive->close(); return false; }
        $data = stream_get_contents($stream);
        fclose($stream);
        $stat = $archive->statName($entry);
        $archive->close();
        if ($data === false || $stat === false) { return false; }
        $this->__elephc_data = (string) $data;
        $this->__elephc_position = 0;
        $this->__elephc_stat = (array) $stat;
        $this->__elephc_seekable = (int) $stat['comp_method'] === ZipArchive::CM_STORE;
        return true;
    }
    public function stream_read(int $count): string|false {
        $chunk = substr($this->__elephc_data, $this->__elephc_position, $count);
        $this->__elephc_position = $this->__elephc_position + strlen($chunk);
        return $chunk;
    }
    public function stream_eof(): bool {
        return $this->__elephc_position >= strlen($this->__elephc_data);
    }
    public function stream_tell(): int {
        return $this->__elephc_position;
    }
    public function stream_seek(int $offset, int $whence = 0): bool {
        if (!$this->__elephc_seekable) {
            __elephc_builtin_warning('fseek(): Stream does not support seeking');
            return false;
        }
        if ($whence === 1) {
            $offset = $offset + $this->__elephc_position;
        } elseif ($whence === 2) {
            $offset = $offset + strlen($this->__elephc_data);
        }
        if ($offset < 0) { return false; }
        $this->__elephc_position = $offset;
        return true;
    }
    public function stream_stat(): array|false {
        // php-src's zip stream stats by reopening the archive path, so replacing the
        // backing file with a non-zip makes fstat() fail even though the descriptor
        // still holds the entry snapshot.
        $archive = new ZipArchive();
        $opened = $archive->open($this->__elephc_archive_name, ZipArchive::RDONLY);
        if ($opened !== true) { return false; }
        $stat = $archive->statName($this->__elephc_entry, ZipArchive::FL_NOCASE);
        $archive->close();
        if ($stat === false) { return false; }
        $this->__elephc_stat = (array) $stat;
        $size = (int) ($this->__elephc_stat['size'] ?? 0);
        $mtime = (int) ($this->__elephc_stat['mtime'] ?? time());
        $mode = 32768;
        return [0 => 0, 1 => -1, 2 => $mode, 3 => 1, 4 => 0, 5 => 0, 6 => -1, 7 => $size, 8 => $mtime, 9 => $mtime, 10 => $mtime, 11 => -1, 12 => -1, 'dev' => 0, 'ino' => -1, 'mode' => $mode, 'nlink' => 1, 'uid' => 0, 'gid' => 0, 'rdev' => -1, 'size' => $size, 'atime' => $mtime, 'mtime' => $mtime, 'ctime' => $mtime, 'blksize' => -1, 'blocks' => -1];
    }
    public function stream_close(): void {}
    public function url_stat(string $_path, int $_flags): array|false {
        return false;
    }
}
stream_wrapper_register('zip', '__ElephcZipStreamWrapper');
function zip_open(string $filename) {
    __elephc_builtin_deprecated(PHP_VERSION_ID >= 80400 ? 'Function zip_open() is deprecated since 8.0, use ZipArchive::open() instead' : 'Function zip_open() is deprecated');
    if ($filename === '') { throw new \ValueError('zip_open(): Argument #1 ($filename) must not be empty'); }
    // php-src's procedural zip_open() calls libzip directly with flags 0, unlike
    // ZipArchive::open(), which promotes a zero-byte file to ZIP_TRUNCATE. Opening
    // an empty file with RDONLY keeps libzip's ER_NOZIP result while still running
    // ZipArchive::open()'s open_basedir check first.
    $archive = new ZipArchive();
    $flags = is_file($filename) && filesize($filename) === 0 ? ZipArchive::RDONLY : 0;
    $opened = $archive->open($filename, $flags);
    // php-src's procedural zip_open() returns false (not an error code) when
    // open_basedir denies the path, so keep the boolean denial distinct from
    // libzip's numeric open errors.
    if ($opened === false) { return false; }
    if ($opened !== true) { return $opened; }
    $stream = fopen('php://temp', 'r+');
    if ($stream === false) { return 12; }
    __ElephcZipProcedural::$state[get_resource_id($stream)] = [$archive, 0, []];
    return $stream;
}
function zip_read($zip) {
    __elephc_builtin_deprecated(PHP_VERSION_ID >= 80400 ? 'Function zip_read() is deprecated since 8.0, use ZipArchive::statIndex() instead' : 'Function zip_read() is deprecated');
    $id = get_resource_id($zip);
    if (!isset(__ElephcZipProcedural::$state[$id])) { throw new \TypeError('zip_read(): supplied resource is not a valid Zip Directory resource'); }
    $state = __ElephcZipProcedural::$state[$id];
    $archive = $state[0];
    $index = $state[1];
    if ($index >= $archive->numFiles) { return false; }
    __ElephcZipProcedural::$state[$id][1] = $index + 1;
    $entry = fopen('php://temp', 'r+');
    if ($entry === false) { return false; }
    __ElephcZipProcedural::$state[$id][2][get_resource_id($entry)] = [$archive, $index, false, 0];
    return $entry;
}
function zip_close($zip): void {
    __elephc_builtin_deprecated(PHP_VERSION_ID >= 80400 ? 'Function zip_close() is deprecated since 8.0, use ZipArchive::close() instead' : 'Function zip_close() is deprecated');
    $id = get_resource_id($zip);
    if (!isset(__ElephcZipProcedural::$state[$id])) { throw new \TypeError('zip_close(): supplied resource is not a valid Zip Directory resource'); }
    $archive = __ElephcZipProcedural::$state[$id][0];
    __ElephcZipProcedural::$state[$id] = null;
    __ElephcZipProcedural::closeResource($zip);
    $archive->close();
}
function zip_entry_open($zip_dp, $zip_entry, string $mode = 'rb'): bool {
    __elephc_builtin_deprecated(PHP_VERSION_ID >= 80400 ? 'Function zip_entry_open() is deprecated since 8.0' : 'Function zip_entry_open() is deprecated');
    $_unusedMode = $mode;
    $zip_id = get_resource_id($zip_dp);
    $entry_id = get_resource_id($zip_entry);
    if (!isset(__ElephcZipProcedural::$state[$zip_id][2][$entry_id])) { return false; }
    __ElephcZipProcedural::$state[$zip_id][2][$entry_id][2] = true;
    return true;
}
function zip_entry_close($zip_entry): bool {
    __elephc_builtin_deprecated(PHP_VERSION_ID >= 80400 ? 'Function zip_entry_close() is deprecated since 8.0' : 'Function zip_entry_close() is deprecated');
    $entry_id = get_resource_id($zip_entry);
    foreach (__ElephcZipProcedural::$state as $zip_id => $state) {
        $state = (array) $state;
        $entries = (array) ($state[2] ?? []);
        if (isset($entries[$entry_id])) {
            __ElephcZipProcedural::$state[$zip_id][2][$entry_id] = null;
            __ElephcZipProcedural::closeResource($zip_entry);
            return true;
        }
    }
    throw new \TypeError('zip_entry_close(): supplied resource is not a valid Zip Entry resource');
}
function zip_entry_read($zip_entry, int $len = 1024): string|false {
    __elephc_builtin_deprecated(PHP_VERSION_ID >= 80400 ? 'Function zip_entry_read() is deprecated since 8.0, use ZipArchive::getFromIndex() instead' : 'Function zip_entry_read() is deprecated');
    $entry_id = get_resource_id($zip_entry);
    foreach (__ElephcZipProcedural::$state as $_zip_id => $state) {
        $state = (array) $state;
        $entries = (array) ($state[2] ?? []);
        if (isset($entries[$entry_id])) {
            $entry_state = (array) $entries[$entry_id];
            $archive = $state[0];
            $index = $entry_state[1];
            $offset = $entry_state[3];
            $data = $archive->getFromIndex($index);
            if ($data === false) { return false; }
            if ($len <= 0) { $len = 1024; }
            $chunk = substr($data, $offset, $len);
            __ElephcZipProcedural::$state[$_zip_id][2][$entry_id][3] = $offset + strlen($chunk);
            return $chunk;
        }
    }
    throw new \TypeError('zip_entry_read(): supplied resource is not a valid Zip Entry resource');
}
function zip_entry_name($zip_entry): string|false {
    __elephc_builtin_deprecated(PHP_VERSION_ID >= 80400 ? 'Function zip_entry_name() is deprecated since 8.0, use ZipArchive::statIndex() instead' : 'Function zip_entry_name() is deprecated');
    $entry_id = get_resource_id($zip_entry);
    foreach (__ElephcZipProcedural::$state as $_zip_id => $state) {
        $state = (array) $state;
        $entries = (array) ($state[2] ?? []);
        if (isset($entries[$entry_id])) {
            $entry_state = (array) $entries[$entry_id];
            return $entry_state[0]->getNameIndex($entry_state[1]);
        }
    }
    return false;
}
function zip_entry_compressedsize($zip_entry): int|false {
    __elephc_builtin_deprecated(PHP_VERSION_ID >= 80400 ? 'Function zip_entry_compressedsize() is deprecated since 8.0, use ZipArchive::statIndex() instead' : 'Function zip_entry_compressedsize() is deprecated');
    $entry_id = get_resource_id($zip_entry);
    foreach (__ElephcZipProcedural::$state as $_zip_id => $state) {
        $state = (array) $state;
        $entries = (array) ($state[2] ?? []);
        if (isset($entries[$entry_id])) {
            $entry_state = (array) $entries[$entry_id];
            $stat = $entry_state[0]->statIndex($entry_state[1]);
            return $stat === false ? false : $stat['comp_size'];
        }
    }
    throw new \TypeError('zip_entry_compressedsize(): supplied resource is not a valid Zip Entry resource');
}
function zip_entry_filesize($zip_entry): int|false {
    __elephc_builtin_deprecated(PHP_VERSION_ID >= 80400 ? 'Function zip_entry_filesize() is deprecated since 8.0, use ZipArchive::statIndex() instead' : 'Function zip_entry_filesize() is deprecated');
    $entry_id = get_resource_id($zip_entry);
    foreach (__ElephcZipProcedural::$state as $_zip_id => $state) {
        $state = (array) $state;
        $entries = (array) ($state[2] ?? []);
        if (isset($entries[$entry_id])) {
            $entry_state = (array) $entries[$entry_id];
            $stat = $entry_state[0]->statIndex($entry_state[1]);
            return $stat === false ? false : $stat['size'];
        }
    }
    throw new \TypeError('zip_entry_filesize(): supplied resource is not a valid Zip Entry resource');
}
function zip_entry_compressionmethod($zip_entry): string|false {
    __elephc_builtin_deprecated(PHP_VERSION_ID >= 80400 ? 'Function zip_entry_compressionmethod() is deprecated since 8.0, use ZipArchive::statIndex() instead' : 'Function zip_entry_compressionmethod() is deprecated');
    $names = [0 => 'stored', 1 => 'shrunk', 2 => 'reduced', 3 => 'reduced', 4 => 'reduced', 5 => 'reduced', 6 => 'imploded', 7 => 'tokenized', 8 => 'deflated', 9 => 'deflatedX', 10 => 'implodedX'];
    $entry_id = get_resource_id($zip_entry);
    foreach (__ElephcZipProcedural::$state as $_zip_id => $state) {
        $state = (array) $state;
        $entries = (array) ($state[2] ?? []);
        if (isset($entries[$entry_id])) {
            $entry_state = (array) $entries[$entry_id];
            $stat = $entry_state[0]->statIndex($entry_state[1]);
            return $stat === false ? false : ($names[$stat['comp_method']] ?? false);
        }
    }
    return false;
}
