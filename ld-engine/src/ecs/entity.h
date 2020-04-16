#ifndef __ENTITY_H_
#define __ENTITY_H_
#include <unordered_map>
#include <vector>

namespace ecs{
    struct GenerativeIndex;


    class EntityManager {
    public:
        EntityManager(){};
        ~EntityManager();

        bool entity_exists(unsigned int entity);
        unsigned int add_entity();
        void remove_entity(unsigned int entity);
        void add_tag(unsigned int entity, unsigned int tagId);
        void remove_tag(unsigned int entity, unsigned int tagId);
        const std::vector<unsigned int>* get_tags(unsigned int entity);

    private:
        unsigned int m_next_entity_id = 0;
        std::unordered_map<unsigned int, std::vector<GenerativeIndex>*> m_components;
        std::unordered_map<unsigned int, std::vector<unsigned int>*> m_tags;
    };
}
#endif
