
using System.Diagnostics;
using System.Reflection;
using AutoFixture;
using FakeItEasy;
using FlexibleConfigEngine.CLI;
using FlexibleConfigEngine.Core.Helper;
using Xunit;

namespace FlexibleConfigUnitTest.CLI
{
    public class VersionTest
    {
        [Fact]
        public void When_CallingVersion_Should_ReportExeVersion()
        {
            //Arrange
            var fixture = Test.Fixture;
            var console = fixture.Freeze<IConsole>();
            var sut = fixture.Create <Version>();

            //Act
            sut.ProcessCommand(new string[] {});

            //Assert
            A.CallTo(() => console.Information(A<string>.Ignored))
                .WhenArgumentsMatch(args => args[0].ToString() == "Version: {FileVersion}").MustHaveHappened();
            A.CallTo(() => console.Information(A<string>.Ignored))
                .WhenArgumentsMatch(args => args[0].ToString() == "ProductVersion: {ProductVersion}").MustHaveHappened();

        }
    }
}
