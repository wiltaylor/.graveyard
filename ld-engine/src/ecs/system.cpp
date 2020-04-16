#include "system.h"

static unsigned int next_id = 0;

unsigned int ecs::SystemBase::nextID() {
    return next_id++;;
}
