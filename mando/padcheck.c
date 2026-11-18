/* Lists the game controllers SDL3 sees and prints what is pressed, for a few seconds. */
#include <SDL3/SDL.h>
#include <stdio.h>
#include <stdlib.h>

int main(int argc, char **argv)
{
    int seconds = argc > 1 ? atoi(argv[1]) : 10, seen = 0, events = 0;
    int far[SDL_GAMEPAD_AXIS_COUNT] = { 0 };
    setvbuf(stdout, NULL, _IOLBF, 0);
    SDL_SetHint(SDL_HINT_JOYSTICK_ALLOW_BACKGROUND_EVENTS, "1");
    if (!SDL_Init(SDL_INIT_GAMEPAD)) {
        printf("SDL_Init: %s\n", SDL_GetError());
        return 1;
    }
    printf("SDL %d.%d.%d\n", SDL_VERSIONNUM_MAJOR(SDL_GetVersion()), SDL_VERSIONNUM_MINOR(SDL_GetVersion()),
           SDL_VERSIONNUM_MICRO(SDL_GetVersion()));
    Uint64 end = SDL_GetTicks() + (Uint64)seconds * 1000;
    while (SDL_GetTicks() < end) {
        SDL_Event e;
        while (SDL_PollEvent(&e)) {
            if (e.type == SDL_EVENT_GAMEPAD_ADDED) {
                SDL_Gamepad *pad = SDL_OpenGamepad(e.gdevice.which);
                if (!pad) {
                    printf("no pude abrir el mando: %s\n", SDL_GetError());
                    continue;
                }
                seen++;
                printf("mando: %s | tipo %s | %04x:%04x\n", SDL_GetGamepadName(pad),
                       SDL_GetGamepadStringForType(SDL_GetGamepadType(pad)), SDL_GetGamepadVendor(pad),
                       SDL_GetGamepadProduct(pad));
                SDL_RumbleGamepad(pad, 0x5000, 0x5000, 400);
            } else if (e.type == SDL_EVENT_GAMEPAD_BUTTON_DOWN) {
                events++;
                printf("boton %s\n", SDL_GetGamepadStringForButton((SDL_GamepadButton)e.gbutton.button));
            } else if (e.type == SDL_EVENT_GAMEPAD_AXIS_MOTION && e.gaxis.axis < SDL_GAMEPAD_AXIS_COUNT) {
                int now = e.gaxis.value > 28000 || e.gaxis.value < -28000;
                if (now && !far[e.gaxis.axis]) {
                    events++;
                    printf("eje %s %d\n", SDL_GetGamepadStringForAxis((SDL_GamepadAxis)e.gaxis.axis), e.gaxis.value);
                }
                far[e.gaxis.axis] = now;
            }
        }
        SDL_Delay(5);
    }
    printf("mandos vistos: %d | pulsaciones leidas: %d\n", seen, events);
    SDL_Quit();
    return 0;
}
