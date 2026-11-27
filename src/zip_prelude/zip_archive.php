<?php

extern "elephc_zip" {
    function elephc_zip_open_string(string $data, int $length): int;
    function elephc_zip_close(int $handle): void;
    function elephc_zip_close_string(int $handle): ptr;
    function elephc_zip_last_len(): int;
    function elephc_zip_num_files(int $handle): int;
    function elephc_zip_is_dirty(int $handle): int;
    function elephc_zip_get_name(int $handle, int $index): ptr;
    function elephc_zip_locate_name(int $handle, string $name, int $length, int $flags): int;
    function elephc_zip_get_index(int $handle, int $index): ptr;
    function elephc_zip_stat(int $handle, int $index, int $field): int;
    function elephc_zip_get_comment_index(int $handle, int $index): ptr;
    function elephc_zip_get_archive_comment(int $handle): ptr;
    function elephc_zip_add_from_string(int $handle, string $name, int $nameLength, string $data, int $dataLength, int $flags, int $mtime): int;
    function elephc_zip_add_empty_dir(int $handle, string $name, int $length, int $flags, int $mtime): int;
    function elephc_zip_delete_index(int $handle, int $index): int;
    function elephc_zip_delete_name(int $handle, string $name, int $length): int;
    function elephc_zip_rename_index(int $handle, int $index, string $name, int $length): int;
    function elephc_zip_rename_name(int $handle, string $old, int $oldLength, string $new, int $newLength): int;
    function elephc_zip_set_archive_comment(int $handle, string $comment, int $length): int;
    function elephc_zip_set_comment_index(int $handle, int $index, string $comment, int $length): int;
    function elephc_zip_set_comment_name(int $handle, string $name, int $nameLength, string $comment, int $commentLength): int;
    function elephc_zip_set_compression_index(int $handle, int $index, int $method): int;
    function elephc_zip_set_compression_name(int $handle, string $name, int $length, int $method): int;
    function elephc_zip_set_mtime_index(int $handle, int $index, int $timestamp): int;
    function elephc_zip_set_mtime_name(int $handle, string $name, int $length, int $timestamp): int;
}

class ZipArchive implements Countable
{
    public const int CREATE = 1;
    public const int EXCL = 2;
    public const int CHECKCONS = 4;
    public const int OVERWRITE = 8;
    public const int RDONLY = 16;

    public const int FL_NOCASE = 1;
    public const int FL_NODIR = 2;
    public const int FL_COMPRESSED = 4;
    public const int FL_UNCHANGED = 8;
    public const int FL_RECOMPRESS = 16;
    public const int FL_ENCRYPTED = 32;
    public const int FL_ENC_GUESS = 0;
    public const int FL_ENC_RAW = 64;
    public const int FL_ENC_STRICT = 128;
    public const int FL_LOCAL = 256;
    public const int FL_CENTRAL = 512;
    public const int FL_ENC_UTF_8 = 2048;
    public const int FL_ENC_CP437 = 4096;
    public const int FL_OVERWRITE = 8192;
    public const int FL_OPEN_FILE_NOW = 16384;

    public const int CM_DEFAULT = -1;
    public const int CM_STORE = 0;
    public const int CM_SHRINK = 1;
    public const int CM_REDUCE_1 = 2;
    public const int CM_REDUCE_2 = 3;
    public const int CM_REDUCE_3 = 4;
    public const int CM_REDUCE_4 = 5;
    public const int CM_IMPLODE = 6;
    public const int CM_DEFLATE = 8;
    public const int CM_DEFLATE64 = 9;
    public const int CM_PKWARE_IMPLODE = 10;
    public const int CM_BZIP2 = 12;
    public const int CM_LZMA = 14;
    public const int CM_TERSE = 18;
    public const int CM_LZ77 = 19;
    public const int CM_LZMA2 = 33;
    public const int CM_ZSTD = 93;
    public const int CM_XZ = 95;
    public const int CM_WAVPACK = 97;
    public const int CM_PPMD = 98;

    public const int ER_OK = 0;
    public const int ER_MULTIDISK = 1;
    public const int ER_RENAME = 2;
    public const int ER_CLOSE = 3;
    public const int ER_SEEK = 4;
    public const int ER_READ = 5;
    public const int ER_WRITE = 6;
    public const int ER_CRC = 7;
    public const int ER_ZIPCLOSED = 8;
    public const int ER_NOENT = 9;
    public const int ER_EXISTS = 10;
    public const int ER_OPEN = 11;
    public const int ER_TMPOPEN = 12;
    public const int ER_ZLIB = 13;
    public const int ER_MEMORY = 14;
    public const int ER_CHANGED = 15;
    public const int ER_COMPNOTSUPP = 16;
    public const int ER_EOF = 17;
    public const int ER_INVAL = 18;
    public const int ER_NOZIP = 19;
    public const int ER_INTERNAL = 20;
    public const int ER_INCONS = 21;
    public const int ER_REMOVE = 22;
    public const int ER_DELETED = 23;
    public const int ER_ENCRNOTSUPP = 24;
    public const int ER_RDONLY = 25;
    public const int ER_NOPASSWD = 26;
    public const int ER_WRONGPASSWD = 27;
    public const int ER_OPNOTSUPP = 28;

    public const int LENGTH_TO_END = 0;
    public const int LENGTH_UNCHECKED = -2;

    public int $lastId = 0;
    public int $status = 0;
    public int $statusSys = 0;
    public int $numFiles = 0;
    public string $filename = "";
    public string $comment = "";

    private int $__handle = 0;
    private bool $__open = false;
    private int $__flags = 0;

    public function open(string $filename, int $flags = 0): bool|int
    {
        $exists = \file_exists($filename);
        if (($flags & self::EXCL) !== 0 && $exists) {
            $this->status = self::ER_EXISTS;
            return self::ER_EXISTS;
        }
        $data = "";
        if (($flags & self::OVERWRITE) === 0) {
            $raw = @\file_get_contents($filename);
            if (!\is_string($raw)) {
                if (($flags & self::CREATE) === 0) {
                    $this->status = self::ER_NOENT;
                    return self::ER_NOENT;
                }
            } else {
                $data = $raw;
            }
        }
        $result = $this->__openData($data, $flags);
        if ($result === true) {
            $this->filename = $filename;
        }
        return $result;
    }

    private function __openData(string $data = "", int $flags = 0): bool|int
    {
        $this->close();
        if (($flags & self::EXCL) !== 0) {
            $this->status = self::ER_EXISTS;
            return self::ER_EXISTS;
        }
        $handle = \elephc_zip_open_string($data, \strlen($data));
        if ($handle < 0) {
            $this->status = self::ER_NOZIP;
            return self::ER_NOZIP;
        }
        $this->__handle = $handle;
        $this->__open = true;
        $this->__flags = $flags;
        $this->numFiles = \elephc_zip_num_files($handle);
        $this->lastId = 0;
        $this->filename = "";
        $this->status = self::ER_OK;
        $this->statusSys = 0;
        $comment = $this->readString(\elephc_zip_get_archive_comment($handle));
        $this->comment = $comment === false ? "" : $comment;
        return true;
    }

    /* __PHP86_BEGIN__ */
    public function openString(string $data = "", int $flags = 0): bool|int
    {
        return $this->__openData($data, $flags);
    }

    public function closeString(): string|false
    {
        if (!$this->__open) {
            return false;
        }
        return $this->__takeBytes();
    }
    /* __PHP86_END__ */

    public function close(): bool
    {
        if (!$this->__open) {
            return true;
        }
        if ($this->filename !== "" && (\elephc_zip_is_dirty($this->__handle) === 1)
            && ($this->__flags & self::RDONLY) === 0) {
            $bytes = $this->__takeBytes();
            if ($bytes === false) {
                $this->status = self::ER_WRITE;
                return false;
            }
            if (@\file_put_contents($this->filename, $bytes) === false) {
                $this->status = self::ER_WRITE;
                return false;
            }
            return true;
        }
        \elephc_zip_close($this->__handle);
        $this->__handle = 0;
        $this->__open = false;
        $this->numFiles = 0;
        $this->__flags = 0;
        return true;
    }

    public function count(): int
    {
        return $this->numFiles;
    }

    public function getStatusString(): string
    {
        switch ($this->status) {
            case self::ER_OK:
                return "No error";
            case self::ER_NOENT:
                return "No such file";
            case self::ER_EXISTS:
                return "File already exists";
            case self::ER_OPEN:
                return "Can't open file";
            case self::ER_COMPNOTSUPP:
                return "Compression method not supported";
            case self::ER_INVAL:
                return "Invalid argument";
            case self::ER_NOZIP:
                return "Not a zip archive";
            case self::ER_ENCRNOTSUPP:
                return "Encryption method not supported";
            case self::ER_RDONLY:
                return "Read-only archive";
            case self::ER_OPNOTSUPP:
                return "Operation not supported";
            case self::ER_WRITE:
                return "Write error";
            default:
                return "Unknown error";
        }
    }

    public function clearError(): void
    {
        $this->status = self::ER_OK;
        $this->statusSys = 0;
    }

    public function getNameIndex(int $index, int $flags = 0): string|false
    {
        unset($flags);
        if (!$this->__open || $index < 0 || $index >= $this->numFiles) {
            return false;
        }
        return $this->readString(\elephc_zip_get_name($this->__handle, $index));
    }

    public function locateName(string $name, int $flags = 0): int|false
    {
        if (!$this->__open) {
            return false;
        }
        $index = \elephc_zip_locate_name($this->__handle, $name, \strlen($name), $flags);
        return $index < 0 ? false : $index;
    }

    public function getFromIndex(int $index, int $len = 0, int $flags = 0): string|false
    {
        unset($flags);
        if (!$this->__open || $index < 0 || $index >= $this->numFiles) {
            return false;
        }
        $data = $this->readString(\elephc_zip_get_index($this->__handle, $index));
        if ($data === false) {
            return false;
        }
        if ($len > 0 && \strlen($data) > $len) {
            return \substr($data, 0, $len);
        }
        return $data;
    }

    public function getFromName(string $name, int $len = 0, int $flags = 0): string|false
    {
        $index = $this->locateName($name, $flags);
        if ($index === false) {
            return false;
        }
        return $this->getFromIndex($index, $len, $flags);
    }

    public function getStreamIndex(int $index, int $flags = 0)
    {
        $data = $this->getFromIndex($index, 0, $flags);
        if ($data === false) {
            return false;
        }
        $stream = \fopen("php://temp", "r+");
        if ($stream === false) {
            return false;
        }
        \fwrite($stream, $data);
        \rewind($stream);
        return $stream;
    }

    public function getStreamName(string $name, int $flags = 0)
    {
        $index = $this->locateName($name, $flags);
        if ($index === false) {
            return false;
        }
        return $this->getStreamIndex($index, $flags);
    }

    public function getStream(string $name)
    {
        return $this->getStreamName($name);
    }

    public function statIndex(int $index, int $flags = 0): array|false
    {
        if (!$this->__open || $index < 0 || $index >= $this->numFiles) {
            return false;
        }
        $name = $this->getNameIndex($index, $flags);
        if ($name === false) {
            return false;
        }
        return [
            "name" => $name,
            "index" => $index,
            "crc" => \elephc_zip_stat($this->__handle, $index, 3),
            "size" => \elephc_zip_stat($this->__handle, $index, 0),
            "mtime" => \elephc_zip_stat($this->__handle, $index, 2),
            "comp_size" => \elephc_zip_stat($this->__handle, $index, 1),
            "comp_method" => \elephc_zip_stat($this->__handle, $index, 4),
            "encryption_method" => 0,
        ];
    }

    public function statName(string $name, int $flags = 0): array|false
    {
        $index = $this->locateName($name, $flags);
        if ($index === false) {
            return false;
        }
        return $this->statIndex($index, $flags);
    }

    public function addEmptyDir(string $dirname, int $flags = 0): bool
    {
        if (!$this->__writable()) {
            return false;
        }
        if (!$this->__open) {
            return false;
        }
        $result = \elephc_zip_add_empty_dir($this->__handle, $dirname, \strlen($dirname), $flags, \time());
        if ($result !== self::ER_OK) {
            $this->status = $result;
            return false;
        }
        $this->refresh();
        $this->lastId = $this->numFiles - 1;
        return true;
    }

    public function addFromString(string $name, string $content, int $flags = self::FL_OVERWRITE): bool
    {
        if (!$this->__writable()) {
            return false;
        }
        if (!$this->__open) {
            return false;
        }
        $result = \elephc_zip_add_from_string($this->__handle, $name, \strlen($name), $content, \strlen($content), $flags, \time());
        if ($result !== self::ER_OK) {
            $this->status = $result;
            return false;
        }
        $this->refresh();
        $this->lastId = $this->numFiles - 1;
        return true;
    }

    public function addFile(string $filepath, string $entryname = "", int $start = 0, int $length = self::LENGTH_TO_END, int $flags = self::FL_OVERWRITE): bool
    {
        $data = @\file_get_contents($filepath);
        if ($data === false) {
            $this->status = self::ER_OPEN;
            return false;
        }
        if ($start > 0) {
            $data = \substr($data, $start);
        }
        if ($length > 0) {
            $data = \substr($data, 0, $length);
        }
        if ($entryname === "") {
            $entryname = $filepath;
        }
        return $this->addFromString($entryname, $data, $flags);
    }

    public function replaceFile(string $filepath, int $index, int $start = 0, int $length = self::LENGTH_TO_END, int $flags = 0): bool
    {
        if (!$this->__writable()) {
            return false;
        }
        unset($flags);
        $name = $this->getNameIndex($index);
        if ($name === false) {
            $this->status = self::ER_INVAL;
            return false;
        }
        $data = @\file_get_contents($filepath);
        if ($data === false) {
            $this->status = self::ER_OPEN;
            return false;
        }
        if ($start > 0) {
            $data = \substr($data, $start);
        }
        if ($length > 0) {
            $data = \substr($data, 0, $length);
        }
        return $this->addFromString($name, $data, self::FL_OVERWRITE);
    }

    public function addGlob(string $pattern, int $flags = 0, array $options = []): array|false
    {
        unset($flags);
        $this->checkGlobOptions("addGlob", $options);
        $files = \glob($pattern);
        if (!\is_array($files)) {
            return false;
        }
        return $this->addGlobbedFiles($files, $options);
    }

    public function addPattern(string $pattern, string $path = ".", array $options = []): array|false
    {
        $this->checkGlobOptions("addPattern", $options);
        $files = \glob(\rtrim($path, "/") . "/" . $pattern);
        if (!\is_array($files)) {
            return false;
        }
        return $this->addGlobbedFiles($files, $options);
    }

    public function deleteIndex(int $index): bool
    {
        if (!$this->__writable()) {
            return false;
        }
        if (!$this->__open) {
            return false;
        }
        $result = \elephc_zip_delete_index($this->__handle, $index);
        if ($result !== self::ER_OK) {
            $this->status = $result;
            return false;
        }
        $this->refresh();
        return true;
    }

    public function deleteName(string $name): bool
    {
        if (!$this->__writable()) {
            return false;
        }
        if (!$this->__open) {
            return false;
        }
        $result = \elephc_zip_delete_name($this->__handle, $name, \strlen($name));
        if ($result !== self::ER_OK) {
            $this->status = $result;
            return false;
        }
        $this->refresh();
        return true;
    }

    public function renameIndex(int $index, string $new_name): bool
    {
        if (!$this->__writable()) {
            return false;
        }
        if (!$this->__open) {
            return false;
        }
        $result = \elephc_zip_rename_index($this->__handle, $index, $new_name, \strlen($new_name));
        if ($result !== self::ER_OK) {
            $this->status = $result;
            return false;
        }
        return true;
    }

    public function renameName(string $name, string $new_name): bool
    {
        if (!$this->__writable()) {
            return false;
        }
        if (!$this->__open) {
            return false;
        }
        $result = \elephc_zip_rename_name($this->__handle, $name, \strlen($name), $new_name, \strlen($new_name));
        if ($result !== self::ER_OK) {
            $this->status = $result;
            return false;
        }
        return true;
    }

    public function setArchiveComment(string $comment): bool
    {
        if (!$this->__writable()) {
            return false;
        }
        if (!$this->__open) {
            return false;
        }
        $result = \elephc_zip_set_archive_comment($this->__handle, $comment, \strlen($comment));
        if ($result !== self::ER_OK) {
            $this->status = $result;
            return false;
        }
        $this->comment = $comment;
        return true;
    }

    public function getArchiveComment(int $flags = 0): string|false
    {
        unset($flags);
        if (!$this->__open) {
            return false;
        }
        return $this->readString(\elephc_zip_get_archive_comment($this->__handle));
    }

    public function setCommentIndex(int $index, string $comment): bool
    {
        if (!$this->__writable()) {
            return false;
        }
        if (!$this->__open) {
            return false;
        }
        $result = \elephc_zip_set_comment_index($this->__handle, $index, $comment, \strlen($comment));
        if ($result !== self::ER_OK) {
            $this->status = $result;
            return false;
        }
        return true;
    }

    public function setCommentName(string $name, string $comment): bool
    {
        if (!$this->__writable()) {
            return false;
        }
        if (!$this->__open) {
            return false;
        }
        $result = \elephc_zip_set_comment_name($this->__handle, $name, \strlen($name), $comment, \strlen($comment));
        if ($result !== self::ER_OK) {
            $this->status = $result;
            return false;
        }
        return true;
    }

    public function getCommentIndex(int $index, int $flags = 0): string|false
    {
        unset($flags);
        if (!$this->__open || $index < 0 || $index >= $this->numFiles) {
            return false;
        }
        return $this->readString(\elephc_zip_get_comment_index($this->__handle, $index));
    }

    public function getCommentName(string $name, int $flags = 0): string|false
    {
        $index = $this->locateName($name, $flags);
        if ($index === false) {
            return false;
        }
        return $this->getCommentIndex($index, $flags);
    }

    public function setCompressionIndex(int $index, int $method, int $compflags = 0): bool
    {
        if (!$this->__writable()) {
            return false;
        }
        unset($compflags);
        if (!$this->__open) {
            return false;
        }
        $result = \elephc_zip_set_compression_index($this->__handle, $index, $method);
        if ($result !== self::ER_OK) {
            $this->status = $result;
            return false;
        }
        return true;
    }

    public function setCompressionName(string $name, int $method, int $compflags = 0): bool
    {
        if (!$this->__writable()) {
            return false;
        }
        unset($compflags);
        if (!$this->__open) {
            return false;
        }
        $result = \elephc_zip_set_compression_name($this->__handle, $name, \strlen($name), $method);
        if ($result !== self::ER_OK) {
            $this->status = $result;
            return false;
        }
        return true;
    }

    public function setMtimeIndex(int $index, int $timestamp, int $flags = 0): bool
    {
        if (!$this->__writable()) {
            return false;
        }
        unset($flags);
        if (!$this->__open) {
            return false;
        }
        $result = \elephc_zip_set_mtime_index($this->__handle, $index, $timestamp);
        if ($result !== self::ER_OK) {
            $this->status = $result;
            return false;
        }
        return true;
    }

    public function setMtimeName(string $name, int $timestamp, int $flags = 0): bool
    {
        if (!$this->__writable()) {
            return false;
        }
        unset($flags);
        if (!$this->__open) {
            return false;
        }
        $result = \elephc_zip_set_mtime_name($this->__handle, $name, \strlen($name), $timestamp);
        if ($result !== self::ER_OK) {
            $this->status = $result;
            return false;
        }
        return true;
    }

    public function extractTo(string $pathto, array|string|null $files = null): bool
    {
        if (!$this->__open) {
            return false;
        }
        $selected = null;
        if ($files !== null) {
            $selected = \is_array($files) ? $files : [$files];
            foreach ($selected as $entry) {
                if (!\is_string($entry)) {
                    throw new \TypeError("ZipArchive::extractTo(): Argument #2 (\$files) must contain only strings");
                }
            }
        }
        $ok = true;
        for ($index = 0; $index < $this->numFiles; $index++) {
            $name = $this->getNameIndex($index);
            if ($name === false) {
                $ok = false;
                continue;
            }
            if ($selected !== null && !\in_array($name, $selected, true)) {
                continue;
            }
            $safe = $this->safeRelativePath($name);
            if ($safe === false) {
                $ok = false;
                continue;
            }
            $target = \rtrim($pathto, "/");
            if ($safe !== "") {
                $target = $target . "/" . $safe;
            }
            if (\substr($safe, -1) === "/") {
                $created = \rtrim($target, "/");
                if ($created !== "" && !\is_dir($created)
                    && !@\mkdir($created, 0777, true) && !\is_dir($created)) {
                    $ok = false;
                }
                continue;
            }
            $directory = \dirname($target);
            if ($directory !== "" && !\is_dir($directory)
                && !@\mkdir($directory, 0777, true) && !\is_dir($directory)) {
                $ok = false;
                continue;
            }
            $data = $this->getFromIndex($index);
            if ($data === false || @\file_put_contents($target, $data) === false) {
                $ok = false;
            }
        }
        return $ok;
    }

    public function setPassword(string $password): bool
    {
        unset($password);
        $this->status = self::ER_ENCRNOTSUPP;
        return false;
    }

    public function setEncryptionIndex(int $index, int $method, ?string $password = null): bool
    {
        unset($index, $method, $password);
        $this->status = self::ER_ENCRNOTSUPP;
        return false;
    }

    public function setEncryptionName(string $name, int $method, ?string $password = null): bool
    {
        unset($name, $method, $password);
        $this->status = self::ER_ENCRNOTSUPP;
        return false;
    }

    public static function isCompressionMethodSupported(int $method, bool $enc = true): bool
    {
        unset($enc);
        return $method === self::CM_STORE || $method === self::CM_DEFLATE;
    }

    public static function isEncryptionMethodSupported(int $method, bool $enc = true): bool
    {
        unset($method, $enc);
        return false;
    }

    public function registerProgressCallback(float $rate, callable $callback): bool
    {
        unset($rate, $callback);
        $this->status = self::ER_OPNOTSUPP;
        return false;
    }

    public function registerCancelCallback(callable $callback): bool
    {
        unset($callback);
        $this->status = self::ER_OPNOTSUPP;
        return false;
    }

    public function setExternalAttributesName(string $name, int $opsys, int $attr, int $flags = 0): bool
    {
        unset($name, $opsys, $attr, $flags);
        $this->status = self::ER_OPNOTSUPP;
        return false;
    }

    public function setExternalAttributesIndex(int $index, int $opsys, int $attr, int $flags = 0): bool
    {
        unset($index, $opsys, $attr, $flags);
        $this->status = self::ER_OPNOTSUPP;
        return false;
    }

    public function getExternalAttributesName(string $name, &$opsys, &$attr, int $flags = 0): bool
    {
        unset($name, $flags);
        $this->status = self::ER_OPNOTSUPP;
        return false;
    }

    public function getExternalAttributesIndex(int $index, &$opsys, &$attr, int $flags = 0): bool
    {
        unset($index, $flags);
        $this->status = self::ER_OPNOTSUPP;
        return false;
    }

    public function setArchiveFlag(int $flag, int $value): bool
    {
        unset($flag, $value);
        $this->status = self::ER_OPNOTSUPP;
        return false;
    }

    public function getArchiveFlag(int $flag, int $flags = 0): int
    {
        unset($flag, $flags);
        return 0;
    }

    public function unchangeAll(): bool
    {
        $this->status = self::ER_OPNOTSUPP;
        return false;
    }

    public function unchangeArchive(): bool
    {
        $this->status = self::ER_OPNOTSUPP;
        return false;
    }

    public function unchangeIndex(int $index): bool
    {
        unset($index);
        $this->status = self::ER_OPNOTSUPP;
        return false;
    }

    public function unchangeName(string $name): bool
    {
        unset($name);
        $this->status = self::ER_OPNOTSUPP;
        return false;
    }

    public function __serialize(): array
    {
        throw new \Exception("Serialization of 'ZipArchive' is not allowed");
    }

    public function __unserialize(array $data): void
    {
        unset($data);
        throw new \Exception("Unserialization of 'ZipArchive' is not allowed");
    }

    public function __destruct()
    {
        $this->close();
    }

    private function __takeBytes(): string|false
    {
        $pointer = \elephc_zip_close_string($this->__handle);
        $this->__handle = 0;
        $this->__open = false;
        $this->numFiles = 0;
        $this->__flags = 0;
        if (\__elephc_ptr_is_null($pointer)) {
            $this->status = self::ER_WRITE;
            return false;
        }
        return \__elephc_ptr_read_string($pointer, \elephc_zip_last_len());
    }

    private function __writable(): bool
    {
        if (($this->__flags & self::RDONLY) !== 0) {
            $this->status = self::ER_RDONLY;
            return false;
        }
        return true;
    }

    private function readString(ptr $pointer): string|false
    {
        if (\__elephc_ptr_is_null($pointer)) {
            return false;
        }
        return \__elephc_ptr_read_string($pointer, \elephc_zip_last_len());
    }

    private function refresh(): void
    {
        if ($this->__open) {
            $this->numFiles = \elephc_zip_num_files($this->__handle);
        }
    }

    private function safeRelativePath(string $name): string|false
    {
        $name = \ltrim(\str_replace("\\", "/", $name), "/");
        $parts = [];
        foreach (\explode("/", $name) as $part) {
            if ($part === "" || $part === ".") {
                continue;
            }
            if ($part === "..") {
                return false;
            }
            $parts[] = $part;
        }
        $joined = \implode("/", $parts);
        if (\substr($name, -1) === "/") {
            $joined = $joined . "/";
        }
        return $joined;
    }

    private function checkGlobOptions(string $method, array $options): void
    {
        if (\array_key_exists("remove_all_path", $options) && !\is_bool($options["remove_all_path"])) {
            throw new \TypeError("ZipArchive::" . $method . "(): Option \"remove_all_path\" must be of type bool");
        }
        foreach (["comp_method", "comp_flags", "enc_method"] as $key) {
            if (!\array_key_exists($key, $options)) {
                continue;
            }
            $value = $options[$key];
            if (!\is_int($value) && !(\is_string($value) && \is_numeric($value))) {
                throw new \TypeError("ZipArchive::" . $method . "(): Option \"" . $key . "\" must be of type int");
            }
        }
    }

    private function addGlobbedFiles(array $files, array $options): array|false
    {
        $addPath = isset($options["add_path"]) ? (string) $options["add_path"] : "";
        $removePath = isset($options["remove_path"]) ? (string) $options["remove_path"] : "";
        $removeAll = isset($options["remove_all_path"]) ? $options["remove_all_path"] : false;
        $method = isset($options["comp_method"]) ? (int) $options["comp_method"] : self::CM_DEFAULT;
        $added = [];
        foreach ($files as $file) {
            $entry = $file;
            if ($removeAll) {
                $entry = \basename($file);
            } elseif ($removePath !== "" && \substr($file, 0, \strlen($removePath)) === $removePath) {
                $entry = \ltrim(\substr($file, \strlen($removePath)), "/");
            }
            if ($addPath !== "") {
                $entry = \rtrim($addPath, "/") . "/" . $entry;
            }
            if (!$this->addFile($file, $entry, 0, self::LENGTH_TO_END, self::FL_OVERWRITE)) {
                return false;
            }
            if ($method === self::CM_STORE || $method === self::CM_DEFLATE) {
                if (!$this->setCompressionName($entry, $method)) {
                    return false;
                }
            }
            $added[] = $entry;
        }
        return $added;
    }
}
