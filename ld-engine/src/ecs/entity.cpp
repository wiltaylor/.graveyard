#include "entity.h"
#include "component.h"
#include <cstdio>
#include <vector>
#include <iterator>
#include <algorithm>

ecs::EntityManager::~EntityManager() {
    for(auto it = m_components.begin(); it != m_components.end(); ++it){
        delete it->second;
    }
}

bool ecs::EntityManager::entity_exists(unsigned int entity) {
    return m_components.find(entity) != m_components.end();
}

unsigned int ecs::EntityManager::add_entity() {
    auto entity = m_next_entity_id++;

    m_components.insert(std::pair<unsigned int, std::vector<GenerativeIndex>*>(entity, new std::vector<GenerativeIndex>));
    m_tags.insert(std::pair<unsigned int, std::vector<unsigned int>*>(entity, new std::vector<unsigned int>));

    return entity;
}

void ecs::EntityManager::remove_entity(unsigned int entity) {
    auto it = m_components.find(entity);

    if(it != m_components.end())
        m_components.erase(it);

    auto it_tag = m_tags.find(entity);

    if(it_tag != m_tags.end())
        m_tags.erase(it_tag);

}

void ecs::EntityManager::add_tag(unsigned int entity, unsigned int tagId) {
    auto it = m_tags.find(entity);

    if(it != m_tags.end()){
        it->second->push_back(tagId);
    }
}

void ecs::EntityManager::remove_tag(unsigned int entity, unsigned int tagId) {
    auto it = m_tags.find(entity);

    if(it != m_tags.end()){
        auto it_tags = std::find(it->second->begin(), it->second->end(), tagId);

        if(it_tags != it->second->end())
            it->second->erase(it_tags);
       
    }
}

const std::vector<unsigned int>* ecs::EntityManager::get_tags(unsigned int entity) {
    auto it = m_tags.find(entity);

    if(it != m_tags.end()){
        return it->second;
    }

    return nullptr;
}
