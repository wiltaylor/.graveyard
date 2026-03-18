using System;
using FluentTest.Exceptions;
using FluentTest.UnitTest.TestObjects;
using Ploeh.AutoFixture;
using Ploeh.AutoFixture.AutoFakeItEasy;
using Xunit;

namespace FluentTest.UnitTest
{
    public class ActTargetTests
    {
        [Fact]
        public void When_InteractingWithSutFromAct_Should_SameObjectInInfoObject()
        {
            var fixture = new Fixture().Customize(new AutoFakeItEasyCustomization());
            var info = fixture.Freeze<TestInfo<IFakeContainer, string, object, object>>();
            var sut = fixture.Create<ActTarget<IFakeContainer, string, object, object>>();

            sut.Act(c => c.Sut = "test");

            Assert.True(info.Sut == "test");
        }

        [Fact]
        public void When_ActAndAssertWithExceptionThrowsExpectedException_Should_NotThrow()
        {
            var fixture = new Fixture().Customize(new AutoFakeItEasyCustomization());
            var sut = fixture.Create<ActTarget<IFakeContainer, string, object, object>>();

            sut.ActAndAssertThrows<ApplicationException>(c => { throw new ApplicationException("expected exception!"); });           
        }

        [Fact]
        public void When_ActAndAssertWithExceptionThrowsUnExpectedException_Should_Throw()
        {
            var fixture = new Fixture().Customize(new AutoFakeItEasyCustomization());
            var sut = fixture.Create<ActTarget<IFakeContainer, string, object, object>>();

            Assert.Throws<FluentTestAssertException>(
                () =>
                    sut.ActAndAssertThrows<ApplicationException>(
                        c => { throw new Exception("expected exception!"); }));
        }

        [Fact]
        public void When_ActAndAssertWithExceptionDoesntThrow_Should_Throw()
        {
            var fixture = new Fixture().Customize(new AutoFakeItEasyCustomization());
            var sut = fixture.Create<ActTarget<IFakeContainer, string, object, object>>();

            Assert.Throws<FluentTestAssertException>(
                () =>
                    sut.ActAndAssertThrows<ApplicationException>(
                        c => { }));
        }

        [Fact]
        public void When_AssertReturnsValue_Should_BeAccessableInAssert()
        {
            var fixture = new Fixture().Customize(new AutoFakeItEasyCustomization());
            var sut = fixture.Create<ActTarget<IFakeContainer, string, object, object>>();

            sut.Act(c => "SomeValue")
                .Assert(c => c.Result == "SomeValue");

        }
    }
}
