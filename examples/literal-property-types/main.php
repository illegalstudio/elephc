<?php

class PendingResult
{
    public false $ready;

    public function __construct(public null $value = null)
    {
        $this->ready = false;
    }

    public static function pending(false $ready): void
    {
        echo "Still pending: ", gettype($ready), "\n";
    }
}

function inspect(mixed $candidate): void
{
    $result = new PendingResult();
    try {
        $result->value = $candidate;
    } catch (TypeError $error) {
        echo $error->getMessage(), "\n";
    }

    try {
        PendingResult::pending($candidate);
    } catch (TypeError $error) {
        echo $error->getMessage(), "\n";
    }
}

inspect(null);
inspect(false);
inspect(0);
