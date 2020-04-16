#ifndef __EVENT_H_
#define __EVENT_H_
#include <unordered_map>

namespace ecs{
    class EventBase {
    public:
        static unsigned int nextID();
    };

    template<typename T>
    class Event{
    public:
        static const unsigned int TypeId;
    };

    template<typename T>
    const unsigned int Event<T>::TypeId(EventBase::nextID());

    class EventManager {
    public:
        EventManager(){}
        ~EventManager(){}

        template<typename T>
        T* get_event() {
            auto it = m_events.find(T::TypeId);

            if(it == m_events.end()){
                T* new_instance = new T();
                m_events.insert(std::pair<unsigned int, EventBase*>(T::TypeId, new_instance));
                return new_instance;
            }

            return static_cast<T*>(it->second);
        }

    private:
        std::unordered_map<unsigned int, EventBase*> m_events;
    };


}
#endif // __EVENT_H_
