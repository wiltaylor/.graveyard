# Getting Started
Welcome to the documentation for FluentTest. Here you will find all the information you need to use this module.

## Requirements
Before you start you will need the following:
- C# 6 (this is the version that comes with Visual Studio 2015).
- .NET 4.5 or higher (only a requirement for your unit test project, project under test can be lower).
- Unit testing framework. XUnit 2 is recommended and will be used in all examples.

## Installation
To install the framework either select it from the nuget gallery or use the following command in the nuget package console.

`Install-Package FluentTest`

## Setting up test project
Before you can start writing tests you need to create a Test Configuration class. This is used to register the container you
will be using to create all of the objects in your test.

THe most common container used is [AutoFixture](https://github.com/AutoFixture/AutoFixture) however you are not limited in what you
can use. You could even create your own fairly easily or another good example is [Type Mock Isolator](https://www.typemock.com/) (though not free).

For all the examples in this guide we will use AutoFixture. Here is an example of a FluentTest configuration using AutoFixture.

```
using Ploeh.AutoFixture;
using Ploeh.AutoFixture.AutoFakeItEasy;

namespace FluentTest.UnitTest
{
    public class Test : TestConfigBase<Fixture>
    {
        public override Fixture BuildContainer()
        {
            new Fixture();
        }
    }
}
```

From the code above you will notice that it is just a matter of changing the generic to whatever object you want to be the container and you return
a new instance of it in BuildContainer().

## Writing your first test
Now the configuration is out of the way you can write your first test. Like any regular unit test you start by creating a public class.

```
public class MyTests 
{

}
``` 

Now instead of writing a normal method for your unit test write it like this:
```
[Fact]
public void MyTestMethod() => new Test().CreateWithSut<TestObject>()
    .Arrange(c => {

    })
    .Act(c => {

    })
    .Assert(c => {

    });
```
Now this looks like a lot happening at first but we will walk through it. First up you will see the `new Test()`. The Test class
is the class we created at the start of this guide. You can call this class whatever you like. I usually call mine Test because it is easy to read.

Next we have the CreateWithSut<TestObject>() method. This is what kicks off the fluent interface. The TestObject set as the generic's type is also
important. This is the type of the SUT (system under test) that you are testing. It is important to set this properly and not just to object if you
want type safety and intellisense in Visual Studio.

Next up there is the 3 chained lambda expressions Arrange, Act and Assert. These are where you will write the 3 parts of your unit tests. In each of these methods you will also notice a 
c variable. This is the test context which contains all the useful properties and methods for dealing with your tests.

### Test Context properties
- Container - This is an instance of the container specified in the Test Configuration Class created at the start of the guide.
- Sut - This is the System Under Test. You need to assign this variable in the Arrange phase.
- Mock - This is the mock object being tested with. This can be left null if not needed. More on mock Objects below.
- Stub - There are 2 method overloads called Stub. You can use Stub("MyStub", value) to set the stub and Stub("MyStub") to retrive it. More details on Mocks and Stubs below.
- Data - This is the current data object assigned to the test. More details on how this works in the Data section below.

Note: When in the Assert phase the Test Context only has the Sut, Mock and Data properties. This is done to encourage better test design.

Ok now lets fill out the Arrange section. We want to create a simple TestObject. So first of all we assign a new TestObject to the Sut property. We will also use the container to
create it.

```
.Arrange(c => {
    c.Sut = c.Container.Create<TestObject>();
})
```

The `Create<TestObject>()` is actually just a standard method of AutoFixture. If you want more information on how it works please review their documentation.

Next up lets call the `SetMyValue(5)` method on our example Sut. This is going to be part of the Act phase as we are running the action we want to test.

```
.Act(c => {
    c.Sut.SetMyValue(5);
})
```

Finally lets verify that MyValue is indeed set to 5 in the finally block.
```
.Assert(c => {
    return c.Sut.MyValue == 5;
});
```

The way assert works is you can either return a boolean value or just not throw an exception. If you pass a boolean value of false. FluentTest will throw a FluentTestAssertException.

So to recap this is what we should now have:

```
public class MyTests 
{
    [Fact]
    public void MyTestMethod() => new Test().CreateWithSut<TestObject>()
        .Arrange(c => {
            c.Sut = c.Container.Create<TestObject>();
        })
        .Act(c => {
            c.Sut.SetMyValue(5);
        })
        .Assert(c => {
            return c.Sut.MyValue == 5;
        });
}
``` 

## Mocks and Stubs
In unit testing you will hear the terms Fakes, Mocks and Stubs thrown around a lot. For the purposes of this framework here are what I define each term:
- Fake - A fake is a test object created to exercise the unit under test (sut). A fake can return any value required to get the system under test to behave in the way desired.
         for the purposes of this guide both Mocks and Stubs are versions of fakes. Fakes are generally created by a Fake/Mocking framework like Moq or FakeItEasy. 
- Mock - A mock is a type of fake as mentioned above. The key difference between a Mock and a Stub, is a Mock will return different values or validate that parts of it are called
         during the test where as a Stub will not.
- Stub - Like a Mock a stub will be created by a Fake/Mocking library but they are generally only used to return default values and prevent null reference exceptions. Think of them as like a shim
         to get the code down the path you want to test. You also can't access Stubs during the Assert phase.

The other limitation of Mocks is you can only have one in a test. This was done intentionally to help encourage better test design. If you are trying to verify multiple objects in a test
it is usually a code smell and good indicator that your test needs to be broken up into multiple tests.

To use Mocks you create your test like before however you use `CreateWithSutAndMock<TestObject, IMockInterface>()` instead of `CreateWithSut<TestObject>()` at the start. You can 
access your mock object off the context like you can with the Sut.

```
.Arrange(c => {
    c.Mock = c.Container.Freeze<IMockInterface>();
    c.Sut = c.Container.Create<TestObject>();
})
```

To create Stub objects you can do so with the Stub method on the context.

```
c.Stub("MyValue", 5);
```

To get the value back again it is just a matter of calling Stub again without the value parameter.
```
int myval = c.Stub("MyValue");
```

## Data 
You might run into instances where you want to run a test with multiple different input values. Most unit testing frameworks have this concept, xUnit calls it Theory. Well good news
is, you can still use your unit testing framework's method of passing data to your test. Anything passed into the method as a parameter is accessable in all blocks.

There is also however a way to do this using FluentTest. The advantage to the FluentTest method is it doesn't use attributes and therefore can use any type of object.

First thing you need to do to use data is change the `CreateWithSut<TestObject>()` to `.CreateWithData<TestObject,int>()` or `CreateWithData<TestObject,IMockInterface,int>()` if you want to
use a mock object. The new type at the end of the declaration (int in this case) is the type of data being used in the test.

```
public void TestMethod() => new Test().CreateWithData<TestObject,IMockInterface,int>()
    .Data(() =>
        {
            return new List<int> {1, 2, 4, 6, 8};
        }, 
    t => t
    .Arrange(c => {
            
        })
    .Act(c => {
            
        })
    .Assert(c => {
            
    }));
```
As you can see there is now a Data block at the top and the Arrange, Act and Assert blocks have been moved inside of t which is the second parameter. The way this works is 
Arrange, Act and Assert will run for each object returned by Data which is an IEnumerable of whatever type you specified as the data type above.

During each phase of the test you can get to the data by using the context object like `c.Data`.