<?php

class User {
    public function __construct(
        public int $id,
        private string $name = "Ada",
        public readonly string $role = "admin",
        public int &$score
    ) {}

    public function label() {
        return $this->name . "#" . $this->id . ":" . $this->role . ":" . $this->score;
    }
}

$score = 10;
$user = new User(7, score: $score);
$score = 12;
echo $user->label();
