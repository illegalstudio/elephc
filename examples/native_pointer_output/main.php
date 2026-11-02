<?php
// posix_memalign writes the allocated address through its first argument.
extern function posix_memalign(ptr<ptr<u8>> $out, int $alignment, int $size): i32;
extern function free(ptr $memory): void;

$slot = buffer_new<ptr<u8>>(1);
if (posix_memalign(buffer_ptr($slot), 8, 16) === 0) {
    $memory = $slot[0];
    ptr_write8($memory, 42);
    echo ptr_read8($memory), "\n";
    free($memory);
}
buffer_free($slot);
