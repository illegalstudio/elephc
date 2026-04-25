<?php

class User {
    public function __construct(
        public int $id,
        private string $name = "Ada",
        public readonly string $role = "admin"
    ) {}

    public function label() {
        return $this->name . "#" . $this->id . ":" . $this->role;
    }
}

$user = new User(7);
echo $user->label();
