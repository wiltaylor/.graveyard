#ifndef __TAGS_H_
#define __TAGS_H_
#include <string>
#include <unordered_map>
namespace ecs{
    class TagManager {
    public:
        TagManager(){}
        ~TagManager(){}

        unsigned int get_tag(std::string name);

        static const unsigned int NULL_TAG = 0;
    private:
        std::unordered_map<std::string, unsigned int> m_tags;
        unsigned int m_next_index = 0;
    };
}
#endif
