<?php

// A custom session store written against SessionHandlerInterface.
//
// The interface is a builtin contract in every compilation mode, so a CLI
// program can implement it, type parameters with it and drive the handler
// directly, for example to unit-test a store before using it under --web.

class MemorySessionHandler implements SessionHandlerInterface
{
    private array $rows = [];

    public function open(string $path, string $name): bool { return true; }
    public function close(): bool { return true; }

    public function read(string $id): string|false
    {
        return $this->rows[$id] ?? "";
    }

    public function write(string $id, string $data): bool
    {
        $this->rows[$id] = $data;
        return true;
    }

    public function destroy(string $id): bool
    {
        $rows = $this->rows;
        unset($rows[$id]);
        $this->rows = $rows;
        return true;
    }

    public function gc(int $max_lifetime): int|false { return 0; }
}

function roundTrip(SessionHandlerInterface $handler, string $id, string $data): string
{
    $handler->open("/tmp", "PHPSESSID");
    $handler->write($id, $data);
    $read = $handler->read($id);
    $handler->close();
    return $read === false ? "(read failed)" : $read;
}

$handler = new MemorySessionHandler();
echo "stored: ", roundTrip($handler, "abc", "user|s:5:\"alice\";"), "\n";
$handler->destroy("abc");
echo "after destroy: '", $handler->read("abc"), "'\n";
echo $handler instanceof SessionHandlerInterface ? "is a SessionHandlerInterface\n" : "not a handler\n";
