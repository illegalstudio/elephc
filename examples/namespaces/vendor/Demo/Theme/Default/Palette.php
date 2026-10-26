<?php

// `Default` is a reserved word, but inside a qualified name it is an ordinary segment,
// as in Laravel Prompts' `Themes\Default` namespace.
namespace Demo\Theme\Default;

final class Palette
{
    public static function accent(): string
    {
        return "teal";
    }
}
