<?php

// Give a callback supplied at runtime access to a message's private data.
class ScopedMessage {
    public function __construct(private string $body) {}
}

function messageReader(callable $reader, ScopedMessage $message): callable {
    return Closure::bind($reader, $message, ScopedMessage::class);
}

$reader = function (string $prefix): string { return $prefix . $this->body; };
$bound = messageReader($reader, new ScopedMessage("compilation complete"));
echo $bound("Status: "), "\n";

// An object can also provide the scope for another bound copy.
$other = $bound->bindTo(new ScopedMessage("ready to run"), new ScopedMessage("scope"));
echo $other("Status: "), "\n";
