<?php

extern "SDL2" {
    function SDL_Init(u32 $flags): i32;
    function SDL_Quit(): void;
    function SDL_CreateWindow(string $title, i32 $x, i32 $y, i32 $w, i32 $h, u32 $flags): ptr;
    function SDL_DestroyWindow(ptr $window): void;
    function SDL_SetWindowTitle(ptr $window, string $title): void;
    function SDL_CreateRenderer(ptr $window, i32 $index, u32 $flags): ptr;
    function SDL_DestroyRenderer(ptr $renderer): void;
    function SDL_RenderSetVSync(ptr $renderer, i32 $vsync): i32;
    function SDL_CreateTexture(ptr $renderer, u32 $format, i32 $access, i32 $w, i32 $h): ptr;
    function SDL_DestroyTexture(ptr $texture): void;
    function SDL_UpdateTexture(ptr $texture, ptr $rect, ptr $pixels, i32 $pitch): i32;
    function SDL_RenderCopy(ptr $renderer, ptr $texture, ptr $src, ptr $dst): i32;
    function SDL_RenderPresent(ptr $renderer): void;
    function SDL_PumpEvents(): void;
    function SDL_GetKeyboardState(ptr $numkeys): ptr;
    function SDL_GetTicks(): u32;
    function SDL_Delay(u32 $ms): void;
    function SDL_GetError(): string;
}
