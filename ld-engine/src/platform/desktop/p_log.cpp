#include "../log.h"
#include <iostream>

void log_info(const char* message) {
    std::cout << "[INFO]: " << message << std::endl;
}

void log_debug(const char* message) {
     std::cout << "[DEBUG]: " << message << std::endl;
}

void log_warn(const char* message) {
     std::cout << "[WARN]: " << message << std::endl;
}

void log_error(const char* message) {
     std::cout << "[ERROR]: " << message << std::endl;
}

void log_fatal(const char* message) {
     std::cout << "[FATAL]: " << message << std::endl;
}
