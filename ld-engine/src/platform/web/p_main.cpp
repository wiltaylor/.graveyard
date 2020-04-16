#include <iostream>
#include <emscripten.h>
#include "../log.h"

void main_loop() {
    log_info("Hellow world!");
}


int main() {
    emscripten_set_main_loop(main_loop, 60, 1);
}
