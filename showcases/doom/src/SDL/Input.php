<?php

namespace Showcases\Doom\SDL;

class Input {
    public function shouldQuit(ptr $keys): int {
        return ptr_read8(ptr_offset($keys, 41));
    }
}
