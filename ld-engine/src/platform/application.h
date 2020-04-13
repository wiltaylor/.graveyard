#ifndef __APPLICATION_H_
#define __APPLICATION_H_
class ApplicationBase {
public:
    ApplicationBase(){}
    virtual ~ApplicationBase() {}

    virtual void onUpdate(float deltaTime) = 0;
};


#endif // __APPLICATION_H_
