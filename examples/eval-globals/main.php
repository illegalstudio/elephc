<?php
$score = 10;

function apply_bonus() {
    global $score;

    if ($score < 0) {
        echo "unexpected\n";
    }

    eval('global $score; $score = $score + 5;');
}

// Superglobals need no `global` statement, inside eval as anywhere else.
function describe_run() {
    eval('echo "arguments=" . $_SERVER["argc"] . "\n";');
}

apply_bonus();
echo "score=" . $score . "\n";
describe_run();

// A runtime-declared reader sees subsequent native writes after its loader returns.
class EnvironmentReaderBase {}

function declare_environment_reader(string $source): void {
    eval($source);
}

$_ENV['EXAMPLE_LEVEL'] = 2;
$readerSource = $argc > 0 ? 'class EnvironmentReader extends EnvironmentReaderBase {
    public function level(): int { return (int) ($_ENV["EXAMPLE_LEVEL"] ?? 0); }
}' : '';
declare_environment_reader($readerSource);
$readerClass = 'EnvironmentReader';
$reader = new $readerClass();
echo 'level=', $reader->level(), "\n";
$_ENV['EXAMPLE_LEVEL'] = 3;
echo 'level=', $reader->level(), "\n";
