<?php
trait StatusLabel {
    private const PREFIX = 'status:';
    public const OWNER = __CLASS__;

    public static function label(): string {
        return self::PREFIX . self::OWNER;
    }
}

enum Status {
    use StatusLabel;
    case Ready;
}

echo Status::OWNER, "|", Status::label(), "\n";
try {
    echo Status::PREFIX;
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
