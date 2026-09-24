<?php
class Settings {
    public int $attempts = 0;
    private string $token = "kept";

    public function token(): string { return $this->token; }
}

function incrementAttempts(mixed $target): void {
    $target->attempts = 3;
}

function overwriteToken(mixed $target): void {
    $target->token = "changed";
}

$settings = new Settings();
incrementAttempts($settings);
echo "attempts: ", $settings->attempts, "\n";

try {
    overwriteToken($settings);
} catch (Error $error) {
    echo "private write blocked\n";
}
echo "token: ", $settings->token(), "\n";
