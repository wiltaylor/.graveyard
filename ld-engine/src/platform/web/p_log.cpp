#include "../log.h"
#include <emscripten.h>

void log_info(const char* message) {
    emscripten_log(EM_LOG_CONSOLE, "[INFO]: %s", message);
}

void log_debug(const char* message) {
     emscripten_log(EM_LOG_CONSOLE | EM_LOG_DEBUG, "[DEBUG]: %s", message);
}

void log_warn(const char* message) {
     emscripten_log(EM_LOG_CONSOLE | EM_LOG_WARN, "[WARN]: %s", message);
}

void log_error(const char* message) {
     emscripten_log(EM_LOG_CONSOLE | EM_LOG_ERROR, "[ERROR]: %s", message);
}

void log_fatal(const char* message) {
     emscripten_log(EM_LOG_CONSOLE | EM_LOG_ERROR, "[FATAL]: %s", message);
}
