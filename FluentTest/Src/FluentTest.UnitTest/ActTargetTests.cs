using FluentTest.UnitTest.TestObjects;
using Ploeh.AutoFixture;
using Ploeh.AutoFixture.AutoFakeItEasy;
using Xunit;

namespace FluentTest.UnitTest
{
    public class ActTargetTests
    {
        [Fact]
        public void When_CallingSutFromSUT_Should_CallUpdateVersionOnTestInfoObject()
        {
            var fixture = new Fixture().Customize(new AutoFakeItEasyCustomization());
            var info = fixture.Freeze<TestInfo<IFakeContainer, string, object, object>>();
            var sut = fixture.Create<ActTarget<IFakeContainer, string, object, object>>();

            sut.Act(c => c.Sut = "test");

            Assert.True(info.Sut == "test");
        }
    }
}
