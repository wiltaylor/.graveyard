using System;
using FluentTest.Exceptions;
using FluentTest.UnitTest.TestObjects;
using Ploeh.AutoFixture;
using Ploeh.AutoFixture.AutoFakeItEasy;
using Xunit;

namespace FluentTest.UnitTest
{
    public class AssertTargetTests
    {
        [Fact]
        public void When_CallingAssertPredicateReturnsFalse_Should_Throw()
        {
            var fixture = new Fixture().Customize(new AutoFakeItEasyCustomization());
            var baseinfo = fixture.Freeze<TestInfo<IFakeContainer, bool, object, object>>();
            var info = fixture.Freeze<AssertTestInfo<IFakeContainer, bool, object, object>>();
            var sut = fixture.Create<AssertTarget<IFakeContainer, bool, object, object>>();
            baseinfo.Sut = false;

            Assert.Throws<FluentTestAssertException>( () =>  sut.Assert(c => c.Sut));
        }

        [Fact]
        public void When_CallingAssertPredicateReturnsTrue_Should_NowThrow()
        {
            var fixture = new Fixture().Customize(new AutoFakeItEasyCustomization());
            var baseinfo = fixture.Freeze<TestInfo<IFakeContainer, bool, object, object>>();
            var info = fixture.Freeze<AssertTestInfo<IFakeContainer, bool, object, object>>();
            var sut = fixture.Create<AssertTarget<IFakeContainer, bool, object, object>>();
            baseinfo.Sut = true;

            sut.Assert(c => c.Sut);
        }

        [Fact]
        public void When_CallingAssertWithoutReturnValue_Should_NotThrow()
        {
            var fixture = new Fixture().Customize(new AutoFakeItEasyCustomization());
            var sut = fixture.Create<AssertTarget<IFakeContainer, bool, object, object>>();

            sut.Assert(c => { });
        }

        [Fact]
        public void When_CallingAssertWithoutReturnValueThatThrows_Should_PassExceptionUpStack()
        {
            var fixture = new Fixture().Customize(new AutoFakeItEasyCustomization());
            var sut = fixture.Create<AssertTarget<IFakeContainer, bool, object, object>>();

            Assert.Throws<Exception>(() => sut.Assert(c => { throw new Exception("Any exception"); }));
        }
    }
}
