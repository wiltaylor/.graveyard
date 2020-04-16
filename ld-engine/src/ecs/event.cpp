#include "event.h"

static unsigned int next_id = 0;

unsigned int nextID() {
    return next_id++;
}
