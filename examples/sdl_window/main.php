<?php
// SDL2 window + renderer + simple interactive loop.
// Run with:
// elephc -l SDL2 -L /opt/homebrew/lib examples/sdl_window/main.php
// ./examples/sdl_window/main

extern "SDL2" {
    function SDL_Init(int $flags): int;
    function SDL_Quit(): void;
    function SDL_CreateWindow(string $title, int $x, int $y, int $w, int $h, int $flags): ptr;
    function SDL_DestroyWindow(ptr $window): void;
    function SDL_CreateRenderer(ptr $window, int $index, int $flags): ptr;
    function SDL_DestroyRenderer(ptr $renderer): void;
    function SDL_SetRenderDrawColor(ptr $renderer, int $r, int $g, int $b, int $a): int;
    function SDL_RenderClear(ptr $renderer): int;
    function SDL_RenderDrawPoint(ptr $renderer, int $x, int $y): int;
    function SDL_RenderPresent(ptr $renderer): void;
    function SDL_PumpEvents(): void;
    function SDL_GetKeyboardState(ptr $numkeys): ptr;
    function SDL_GetTicks(): int;
    function SDL_Delay(int $ms): void;
    function SDL_GetError(): string;
}

$SDL_INIT_VIDEO = 32;
$SDL_RENDERER_ACCELERATED = 2;
$SDL_SCANCODE_ESCAPE = 41;
$SDL_SCANCODE_SPACE = 44;

$width = 640;
$height = 360;
$square = 28;
$centerX = intdiv($width, 2);
$centerY = intdiv($height, 2);

if (SDL_Init($SDL_INIT_VIDEO) != 0) {
    echo "SDL_Init failed: " . SDL_GetError() . "\n";
    exit(1);
}

$window = SDL_CreateWindow("elephc SDL2 demo", 100, 100, $width, $height, 0);
if (ptr_is_null($window)) {
    echo "SDL_CreateWindow failed: " . SDL_GetError() . "\n";
    SDL_Quit();
    exit(1);
}

$renderer = SDL_CreateRenderer($window, -1, $SDL_RENDERER_ACCELERATED);
if (ptr_is_null($renderer)) {
    echo "SDL_CreateRenderer failed: " . SDL_GetError() . "\n";
    SDL_DestroyWindow($window);
    SDL_Quit();
    exit(1);
}

echo "window ok\n";
echo "SPACE or ESC to quit early\n";

$start = SDL_GetTicks();
$duration = 5000;
$frame = 0;

while (SDL_GetTicks() - $start < $duration) {
    SDL_PumpEvents();
    $keys = SDL_GetKeyboardState(ptr_null());
    $escape = ptr_read8(ptr_offset($keys, $SDL_SCANCODE_ESCAPE));
    $space = ptr_read8(ptr_offset($keys, $SDL_SCANCODE_SPACE));

    if ($escape || $space) {
        break;
    }

    // Dark background.
    SDL_SetRenderDrawColor($renderer, 14, 18, 28, 255);
    SDL_RenderClear($renderer);

    // Animated square that bounces around the window.
    $travelX = $width - $square - 40;
    $travelY = $height - $square - 40;
    $cycleX = $travelX * 2;
    $cycleY = $travelY * 2;
    $rawX = ($frame * 4) % $cycleX;
    $rawY = ($frame * 3) % $cycleY;
    $boxX = $rawX < $travelX ? $rawX : $cycleX - $rawX;
    $boxY = $rawY < $travelY ? $rawY : $cycleY - $rawY;
    $boxX += 20;
    $boxY += 20;

    $r = 80 + (($frame * 3) % 120);
    $g = 140 + (($frame * 5) % 80);
    $b = 220 - (($frame * 2) % 100);

    SDL_SetRenderDrawColor($renderer, $r, $g, $b, 255);
    for ($y = 0; $y < $square; $y++) {
        for ($x = 0; $x < $square; $x++) {
            SDL_RenderDrawPoint($renderer, $boxX + $x, $boxY + $y);
        }
    }

    // Small white marker in the center so the frame feels more alive.
    SDL_SetRenderDrawColor($renderer, 255, 255, 255, 255);
    for ($i = -6; $i <= 6; $i++) {
        SDL_RenderDrawPoint($renderer, $centerX + $i, $centerY);
        SDL_RenderDrawPoint($renderer, $centerX, $centerY + $i);
    }

    SDL_RenderPresent($renderer);
    SDL_Delay(16);
    $frame++;
}

SDL_DestroyRenderer($renderer);
SDL_DestroyWindow($window);
SDL_Quit();
