#ifndef __SYSTEM_H_
#define __SYSTEM_H_

#include <unordered_map>

namespace ecs{
    class SystemBase {
    public:
        SystemBase(){}
        virtual ~SystemBase(){}

        virtual void init() = 0;
        virtual void on_enable() = 0;
        virtual void on_disable() = 0;
        static  unsigned int nextID();
        bool enabled;
    };

    template<typename T>
    class System : public SystemBase {
        static const unsigned int TypeId;
        static const size_t SizeInBytes;
    };

    template<typename T>
    const unsigned int System<T>::TypeId(SystemBase::nextID());

    template<typename T>
    const size_t System<T>::SizeInBytes(sizeof(T));

    class SystemManager {
    public:
        SystemManager(){}
        ~SystemManager(){}

        template<typename T>
        void register_system(){
            assert(m_systems.find(T::TypeId) == m_systems.end());

            auto instance = new T();

            m_systems.insert(std::pair<unsigned int, SystemBase*>(T::TypeId, instance));

            instance->init();
        }

        template<typename T>
        void enable(){
            auto it = m_systems.find(T::TypeId);

            if(it != m_systems.end()) {
                auto sys = it->second;
                sys->enabled = true;
                sys->on_enable();

            }
        }

        template<typename T>
        void disable() {
            auto it = m_systems.find(T::TypeId);

            if(it != m_systems.end()) {
                auto sys = it->second;
                sys->enabled = false;
                sys->on_disable();
            }
        }

    private:
        std::unordered_map<unsigned int, SystemBase*> m_systems;
    };
}

#endif
