#include "component.h"
#include <cstddef>
#include <stdlib.h>
#include <string.h>
#include <assert.h>

static unsigned int NextTypeId = 0;

unsigned int ecs::ComponentBase::nextID() {
    return NextTypeId++;
}

ecs::ComponentContainer::ComponentContainer(size_t capacity, size_t item_size, unsigned int type_id): m_capacity{capacity}, m_item_size{item_size}, m_type_id{type_id} {
    m_data = static_cast<char*>(malloc(capacity * item_size));
    m_inuse.reserve(capacity);
    m_generation.reserve(capacity);

    for(auto i = 0; i < capacity; i++) {
        m_inuse.push_back(false);
        m_generation.push_back(0);
    }
}

ecs::ComponentContainer::~ComponentContainer() {
  free(m_data);
}

char* ecs::ComponentContainer::get_component(GenerativeIndex &index) {
    if(index.ordinal >= m_capacity)
        return nullptr;

    if(m_inuse[index.ordinal] == false)
        return nullptr;

    if(m_generation[index.ordinal] != index.generation)
        return nullptr;

    return m_data + m_item_size * index.ordinal;
}

void ecs::ComponentContainer::remove_component(GenerativeIndex &index) {
    if(index.ordinal >= m_capacity)
        return;

    if(m_generation[index.ordinal] != index.generation)
        return;

    m_inuse[index.ordinal] = false;
}

void ecs::ComponentContainer::begin() {
    m_current_ordinal = 0;
}

ecs::GenerativeIndex ecs::ComponentContainer::add_component(unsigned int entity) {
    size_t index = 0;

    while(index < m_capacity){
        if(m_inuse[index] == false) {
            m_inuse[index] = true;
            m_generation[index]++;

            //Zero out memory of component
            memset(m_data + m_item_size * index, 0, m_item_size);

            auto ent = reinterpret_cast<ComponentBase*>(m_data + m_item_size * index);
            ent->Entity = entity;

            return ecs::GenerativeIndex{ index, m_generation[index], m_type_id };
        }
    }

    size_t new_capacity = m_capacity * 2;
    m_data = static_cast<char*>(realloc(m_data, new_capacity * m_item_size));

    assert(m_data != nullptr);

    for(auto i = m_capacity; i < new_capacity; i++) {
        m_inuse.push_back(false);
        m_generation.push_back(0);
    }

    m_capacity = new_capacity;

    return add_component(entity);
}

char* ecs::ComponentContainer::next() {
    while(m_current_ordinal < m_capacity) {
        if(m_inuse[m_current_ordinal] == true) {
            m_current_ordinal++;
            return m_data + m_item_size * m_current_ordinal;
        }

        m_current_ordinal++;
    }

    return nullptr;
}

ecs::ComponentManager::~ComponentManager(){
    for(auto it = m_component_storage.begin(); it != m_component_storage.end(); ++it) {
        delete it->second;
    }
}


void ecs::ComponentManager::remove_component(ecs::GenerativeIndex& index) {
    m_component_storage[index.type_id]->remove_component(index);
}
