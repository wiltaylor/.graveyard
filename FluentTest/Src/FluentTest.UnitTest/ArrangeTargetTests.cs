using FluentTest.UnitTest.TestObjects;
using Ploeh.AutoFixture;
using Ploeh.AutoFixture.AutoFakeItEasy;
using Xunit;

namespace FluentTest.UnitTest
{
    public class ArrangeTargetTests
    {
        [Fact]
        public void When_CallingArrangeStoredSut_Should_BeStoredOnTestInfo()
        {
            var fixture = new Fixture().Customize(new AutoFakeItEasyCustomization());
            var info = fixture.Freeze<TestInfo<IFakeContainer, bool, object, object>>();
            var sut = fixture.Create<ArrangeTarget<IFakeContainer, bool, object, object>>();

            sut.Arrange(c => c.Sut = true);

            Assert.True(info.Sut);
        }

        [Fact]
        public void When_CallingArrangeStoredMock_Should_BeStoredOnTestInfo()
        {
            var fixture = new Fixture().Customize(new AutoFakeItEasyCustomization());
            var info = fixture.Freeze<TestInfo<IFakeContainer, object, bool, object>>();
            var sut = fixture.Create<ArrangeTarget<IFakeContainer, object, bool, object>>();

            sut.Arrange(c => c.Mock = true);

            Assert.True(info.Mock);
        }

        [Fact]
        public void When_SettingStub_Should_BeAbleToRetriveStubWithSameName()
        {
            var fixture = new Fixture().Customize(new AutoFakeItEasyCustomization());
            var info = fixture.Freeze<TestInfo<IFakeContainer, object, object, object>>();
            var sut = fixture.Create<ArrangeTarget<IFakeContainer, object, object, object>>();

            sut.Arrange(c => c.Stub("TestStub", 5));

            Assert.True(info.Stub<int>("TestStub") == 5);
        }
    }
}
