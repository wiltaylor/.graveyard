using System.Collections.Generic;
using FakeItEasy;
using FluentTest.Exceptions;
using FluentTest.UnitTest.TestObjects;
using Ploeh.AutoFixture;
using Xunit;

namespace FluentTest.UnitTest
{
    public class BasicTests
    {
        [Fact]
        public void When_RunningTestWithSutOnly_Should_NotThrow() => new TestConfig().Create<TestSut>()
            .Arrange((container, context) =>
            {
                context.Sut = container.Create<TestSut>();
            })
            .Act(sut =>
            {
                sut.AProperty = "foo";
            })
            .Assert((sut, mock) => sut.AProperty == "foo");

        [Fact]
        public void When_TestDoesntEqualAssert_Should_ThrowFluentTEstAssertException() =>
            Assert.Throws<FluentTestAssertException>(() =>
            {
                new TestConfig().Create<TestSut>()
                    .Arrange((container, context) => context.Sut = container.Create<TestSut>())
                    .Act(sut => sut.AProperty = "WrongValue")
                    .Assert(sut => sut.AProperty == "AnotherValue");
            });

        [Fact]
        public void When_RunningTestWithMock_Should_NotThrow() => new TestConfig().Create<TestSut, IMockInterface>()
            .Arrange((container, context) =>
            {
                context.Mock = container.Freeze<IMockInterface>();
                context.Sut = container.Create<TestSut>();
            })
            .Act(sut => sut.CallMock())
            .Assert((sut, mock) => A.CallTo(() => mock.CallMe()).MustHaveHappened());

        [Fact]
        public void When_RunningTestWithMockThatFailsExpecationCheck_Should_PassExceptionUpStack() =>
            Assert.Throws<ExpectationException>(() => 
                new TestConfig().Create<TestSut, IMockInterface>()
                    .Arrange((container, context) =>
                    {
                        context.Mock = container.Freeze<IMockInterface>();
                        context.Sut = container.Create<TestSut>();
                    })
                    .Act(sut => sut.AProperty = "5")
                    .Assert((sut, mock) => A.CallTo(() => mock.CallMe()).MustHaveHappened()));
        

        [Fact]
        public void When_ComparingMockInAssert_Should_NotThrowIfTrueIsReturned()
            => new TestConfig().Create<TestSut, IMockInterface>()
                .Arrange((container, context) =>
                {
                    context.Mock = container.Freeze<IMockInterface>();
                    context.Sut = container.Create<TestSut>();

                    A.CallTo(() => context.Mock.TestProp).Returns("expected");
                })
                .Act(sut => { } /* any action. */)
                .Assert((sut, mock) => mock.TestProp == "expected");

        [Fact]
        public void When_InteractingWithMockInAct_Should_NoThrow() => new TestConfig().Create<TestSut, IMockInterface>()
                .Arrange((container, context) =>
                {
                    context.Mock = container.Freeze<IMockInterface>();
                    context.Sut = container.Create<TestSut>();

                    A.CallTo(() => context.Mock.TestProp).Returns("expected");
                })
                .Act((sut, mock )=> sut.AProperty = mock.TestProp)
                .Assert(sut => sut.AProperty == "expected");

        [Fact]
        public void When_AssertWithMockReturnsFalse_Should_Throw() =>
            Assert.Throws<FluentTestAssertException>(() =>
                new TestConfig().Create<TestSut, IMockInterface>()
                    .Arrange((container, context) =>
                    {
                        context.Mock = container.Freeze<IMockInterface>();
                        context.Sut = container.Create<TestSut>();
                    })
                    .Act(sut => { } /*any action*/)
                    .Assert((sut, mock) => false));

        [Fact]
        public void When_CallingAssertWithoutReturns_Should_NotThrow() => new TestConfig().Create<TestSut>()
            .Arrange((container, context) => context.Sut = container.Create<TestSut>())
            .Act(sut => sut.AProperty = "anything")
            .Assert(sut => sut.CallMock());

        [Fact]
        public void When_PassingDataIn_Should_BeAccessableInActAndAssert() => new TestConfig().CreateWithData<TestSut,string>()
            .Data(() => new [] {"first", "second"}, 
            t => t
                .Arrange((container, context) => context.Sut = container.Create<TestSut>())
                .Act((sut, mock, data) =>  sut.AProperty = data)
                .Assert((sut, mock, data) => sut.AProperty == data));

        [Fact]
        public void When_PassingDataToAssertThatReturnsFalse_Should_Throw() =>
            Assert.Throws<FluentTestAssertException>( () => 
            new TestConfig().CreateWithData<TestSut, string>()
                .Data(() => new[] {"first", "second"},
                    t => t
                        .Arrange((container, context) => context.Sut = container.Create<TestSut>())
                        .Act((sut, mock, data) => sut.AProperty = data)
                        .Assert((sut, mock, data) => false)));

        [Fact]
        public void When_PassingDataToAssertWithoutReturnData_Should_NotThrow()
            => new TestConfig().CreateWithData<TestSut, string>()
                .Data(() => new [] {"first", "second"},
                    t => t
                        .Arrange((container, context) => context.Sut = container.Create<TestSut>())
                        .Act((sut, mock, data) => { })
                        .Assert((sut, mock, data) => { }));





    }
}
