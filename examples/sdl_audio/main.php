<?php
// Basic SDL2 audio subsystem init.
// Run with:
// elephc -l SDL2 -L /opt/homebrew/lib examples/sdl_audio/main.php

extern "SDL2" {
    function SDL_Init(int $flags): int;
    function SDL_Quit(): void;
    function SDL_GetNumAudioDrivers(): int;
    function SDL_GetAudioDriver(int $index): string;
    function SDL_GetCurrentAudioDriver(): string;
    function SDL_GetError(): string;
}

$SDL_INIT_AUDIO = 16;

if (SDL_Init($SDL_INIT_AUDIO) != 0) {
    echo "SDL_Init failed: " . SDL_GetError() . "\n";
    exit(1);
}

$count = SDL_GetNumAudioDrivers();
echo "audio drivers = " . $count . "\n";
if ($count > 0) {
    echo "first = " . SDL_GetAudioDriver(0) . "\n";
}

$current = SDL_GetCurrentAudioDriver();
echo "current = " . (strlen($current) > 0 ? $current : "none") . "\n";

SDL_Quit();
