<?php

namespace Showcases\Doom\SDL;

class Input {
    public function keyPressed(ptr $keys, int $scanCode): int {
        return ptr_read8(ptr_offset($keys, $scanCode));
    }

    public function shouldQuit(ptr $keys): int {
        return $this->keyPressed($keys, 41);
    }

    public function moveForward(ptr $keys): int {
        return $this->keyPressed($keys, 26);
    }

    public function moveBackward(ptr $keys): int {
        return $this->keyPressed($keys, 22);
    }

    public function moveLeft(ptr $keys): int {
        return $this->keyPressed($keys, 4);
    }

    public function moveRight(ptr $keys): int {
        return $this->keyPressed($keys, 7);
    }

    public function turnLeft(ptr $keys): int {
        return $this->keyPressed($keys, 80);
    }

    public function turnRight(ptr $keys): int {
        return $this->keyPressed($keys, 79);
    }
}
