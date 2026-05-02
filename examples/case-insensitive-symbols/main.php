<?php

INTERFACE Named {
    PUBLIC FUNCTION Label(): string;
}

CLASS Greeter IMPLEMENTS named {
    public string $Name = "Ada";
    public string $name = "Lovelace";

    PUBLIC FUNCTION Label(): string {
        RETURN $this->Name . " " . $this->name;
    }
}

FUNCTION shout(string $value): string {
    RETURN STRTOUPPER($value);
}

$greeter = NEW greeter();

ECHO shout($greeter->LABEL());
ECHO "\n";
ECHO $greeter instanceof NAMED ? "implements Named" : "missing Named";
ECHO "\n";
