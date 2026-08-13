<?php

// Userspace stream wrapper that exposes a virtual, in-memory directory.
//
// Registering a class with stream_wrapper_register() lets opendir(), readdir(),
// rewinddir(), and closedir() iterate entries that never touch the filesystem:
// the wrapper's dir_opendir/dir_readdir/dir_rewinddir/dir_closedir methods are
// called instead. This is the same mechanism PHP uses for things like phar://.

class VirtualDir
{
    public $context;
    private $entries = ["readme.txt", "notes.md", "todo.txt"];
    private $pos = 0;

    public function dir_opendir($path, $options): bool
    {
        $this->pos = 0;
        return true;
    }

    public function dir_readdir(): string|false
    {
        if ($this->pos >= count($this->entries)) {
            return false; // false signals end-of-directory, as PHP documents
        }
        $name = $this->entries[$this->pos];
        $this->pos = $this->pos + 1;
        return $name;
    }

    public function dir_rewinddir(): bool
    {
        $this->pos = 0;
        return true;
    }

    public function dir_closedir(): bool
    {
        return true;
    }
}

stream_wrapper_register("vdir", "VirtualDir");

$dh = opendir("vdir://anything");

echo "First pass:\n";
while (($entry = readdir($dh)) !== false) {
    echo "  - $entry\n";
}

rewinddir($dh);

echo "After rewind, first entry: " . readdir($dh) . "\n";

closedir($dh);
