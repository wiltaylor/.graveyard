#include "tags.h"

unsigned int ecs::TagManager::get_tag(std::string name) {
    auto it = m_tags.find(name);

    if(it == m_tags.end()){
        unsigned int new_index = m_next_index++;

        m_tags.insert(std::pair<std::string, unsigned int>(name, new_index));
        return new_index;
    }

    return m_tags[name];
}
