# Fluent Test
Fluent test is an experimental framework I am working on to try and make the Arrange, Act and Assert phases of unit testing more noticable and readable in unit tests.

The framework also enfroces good pratices by only allowing one mock object during the test.

To illistrate how this works the following is a typical unit test:
```
[Fact]
public void When_SystemIsInExampleState_Should_ReturnExampleState()
{
    var container = new Fixture();
    var sut = container.Create<TestSut>();

    sut.DoSomething();

    Assert.That(sut.State == "Example");
}
```
With this framework it now looks like this:
```
[Fact]
public void When_SystemIsInExampleState_Should_ReturnExampleState() => new TestConfig().Create<TestSut>()
    .Arrange((container, context) =>
    {
        context.Sut = container.Create<TestSut>();
    })
    .Act(sut =>
    {
        sut.DoSomething();
    })
    .Assert((sut, mock) => {
        sut.State == "Example";
    });
```
While the example above is a lot longer code wise I find it  
- Clearer split of code into Arrange, Act and Assert pattern.
- It enforces good practices like only having a single mock object. 


## Installation
TODO: Describe the installation process
## Usage
TODO: Write usage instructions
## Contributing
1. Fork it!
2. Create your feature branch: `git checkout -b my-new-feature`
3. Commit your changes: `git commit -am 'Add some feature'`
4. Push to the branch: `git push origin my-new-feature`
5. Submit a pull request :D
## History
TODO: Write history
## Credits
TODO: Write credits
## License
TODO: Write license
