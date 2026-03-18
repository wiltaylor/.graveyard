# Fluent Test
[![GitHub version](https://badge.fury.io/gh/wiltaylor%2Ffluenttest.svg)](https://badge.fury.io/gh/wiltaylor%2Ffluenttest)
[![Develop Build status](https://ci.appveyor.com/api/projects/status/hk5755di5488xrn9/branch/develop?svg=true&passingText=develop%20-%20OK&failingText=develop%20-%20FAIL&pendingText=develop%20-%20BUILDING)](https://ci.appveyor.com/project/wiltaylor/fluenttest/branch/develop)
[![Master Build status](https://ci.appveyor.com/api/projects/status/hk5755di5488xrn9/branch/master?svg=true&passingText=master%20-%20OK&failingText=master%20-%20FAIL&pendingText=master%20-%20BUILDING)](https://ci.appveyor.com/project/wiltaylor/fluenttest/branch/master)
[![Latest Docs status](https://readthedocs.org/projects/fluenttest/badge/?version=latest)](http://fluenttest.readthedocs.io/en/latest/)
[![Develop Docs status](https://readthedocs.org/projects/fluenttest/badge/?version=develop)](http://fluenttest.readthedocs.io/en/develop/)
[![NuGet version](https://badge.fury.io/nu/fluenttest.svg?label=Nuget)](https://badge.fury.io/nu/fluenttest)
[![MIT license](http://img.shields.io/badge/license-MIT-brightgreen.svg)](http://opensource.org/licenses/MIT)


Fluent test is an experimental framework I am working on to try and make the Arrange, Act and Assert phases of unit testing more readable and also enforces good practice.

To illustrate how this works the following is a typical unit test:
```
[Fact]
public void TyicalExampleTest()
{
    var container = new Fixture().Customize(new AutoFakeItEasyCustomization());
    var sut = container.Create<TestObject>();

    sut.AProperty = "Example";

    Assert.True(sut.AProperty == "Example");
}
```
With this framework it now looks like this:
```
[Fact]
public void TestImplimentedWithFluentTest() => new Test().CreateWithSut<TestObject>()
    .Arrange(c => c.Sut = c.Container.Create<TestObject>())
    .Act(c => c.Sut.AProperty = "Example")
    .Assert(c => c.Sut.AProperty == "Example");
```

## Installation
You can install this package via NuGet in Visual Studio 2015+. 

You can also get a zipped version in the release section of this repository.

## Usage
You can view a guide on how to use this library [here](http://fluenttest.readthedocs.io/en/latest/).

## Contributing
1. Raise a issue in issues for the feature or bug you are fixing.
2. Fork the develop branch.
3. Make your changes. 
4. Make sure to update the docs on the new feature if applicable.
5. Make sure your change has a unit test and all other unit tests pass.
6. Submit a pull request :D

## License
The MIT License (MIT)

Copyright (c) 2015 Wil Taylor

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

