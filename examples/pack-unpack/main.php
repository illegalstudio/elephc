<?php
// pack()/unpack() endianness modifiers are PHP 8.6 syntax: compile with --php-version=8.6.
$record = pack('N', 0xCAFEBABE)
    . pack('s<', -2)
    . pack('e', 3.5);

echo bin2hex($record), PHP_EOL;
print_r(unpack('Nmagic/s<count/evalue', $record));
