using System;
using System.Linq;
using AutoFixture;
using FakeItEasy;
using FlexibleConfigEngine.CLI;
using FlexibleConfigEngine.Core.ConfigSession;
using FlexibleConfigEngine.Core.Helper;
using Xunit;

namespace FlexibleConfigUnitTest.CLI
{
    public class GatherTests
    {
        [Fact]
        public void When_RunningGatherAndNoConfigScriptFound_Should_ReportError()
        {
            //Arrange
            var fixture = Test.Fixture;
            var filesystem = fixture.Freeze<IFileSystem>();
            var console = fixture.Freeze<IConsole>();
            var sut = fixture.Create<FlexibleConfigEngine.CLI.Gather>();

            A.CallTo(() => filesystem.FileExist("config.csx")).Returns(false);


            //Act
            sut.ProcessCommand(new string[]{});

            //Assert
            A.CallTo(() => console.Error(A<string>.Ignored, A<object>.Ignored))
                .WhenArgumentsMatch(args => args[0].ToString().Contains("The configuration script {script} is missing!") && ((Object[])args[1]).First().ToString() == "config.csx")
                .MustHaveHappened();
        }

        [Fact]
        public void When_PassingInScriptSwitch_Should_CheckForSpecifiedScriptFileInstead()
        {
            //Arrange
            var fixture = Test.Fixture;
            var filesystem = fixture.Freeze<IFileSystem>();
            var console = fixture.Freeze<IConsole>();
            var sut = fixture.Create<FlexibleConfigEngine.CLI.Gather>();

            A.CallTo(() => filesystem.FileExist("test.csx")).Returns(false);
            sut.AppendSwitch("script", "test.csx");

            //Act
            sut.ProcessCommand(new string[] { });

            //Assert
            A.CallTo(() => console.Error(A<string>.Ignored, A<object>.Ignored))
                .WhenArgumentsMatch(args => args[0].ToString().Contains("The configuration script {script} is missing!") && ((Object[])args[1]).First().ToString() == "test.csx")
                .MustHaveHappened();
        }

        [Fact]
        public void When_PassingInNoOutputFile_Should_DefaultToGatherJson()
        {
            //Arrange
            var fixture = Test.Fixture;
            var filesystem = fixture.Freeze<IFileSystem>();
            var session = fixture.Freeze<ISession>();
            var sut = fixture.Create<FlexibleConfigEngine.CLI.Gather>();

            A.CallTo(() => filesystem.FileExist("config.csx")).Returns(true);

            //Act
            sut.ProcessCommand(new string[] { });

            //Assert
            A.CallTo(() => session.GatherOnly("config.csx", "gather.json")).MustHaveHappened();
        }

        [Fact]
        public void When_PassingInOutputFileSwitch_Should_PassToSession()
        {
            //Arrange
            var fixture = Test.Fixture;
            var filesystem = fixture.Freeze<IFileSystem>();
            var session = fixture.Freeze<ISession>();
            var sut = fixture.Create<FlexibleConfigEngine.CLI.Gather>();

            sut.AppendSwitch("output", "test.json");

            A.CallTo(() => filesystem.FileExist("config.csx")).Returns(true);

            //Act
            sut.ProcessCommand(new string[] { });

            //Assert
            A.CallTo(() => session.GatherOnly("config.csx", "test.json")).MustHaveHappened();

        }

        [Fact]
        public void When_CallingGatherWithExistingScript_Should_CallSession()
        {
            //Arrange
            var fixture = Test.Fixture;
            var filesystem = fixture.Freeze<IFileSystem>();
            var session = fixture.Freeze<ISession>();
            var sut = fixture.Create<FlexibleConfigEngine.CLI.Gather>();

            A.CallTo(() => filesystem.FileExist("config.csx")).Returns(true);

            //Act
            sut.ProcessCommand(new string[] { });

            //Assert
            A.CallTo(() => session.GatherOnly("config.csx", A<string>.Ignored)).MustHaveHappened();
        }

        [Fact]
        public void When_CallingGatherAndSessionThrows_Should_ReportErrorToConsoleAndNotThrowFurther()
        {
            //Arrange
            var fixture = Test.Fixture;
            var filesystem = fixture.Freeze<IFileSystem>();
            var session = fixture.Freeze<ISession>();
            var console = fixture.Freeze<IConsole>();
            var sut = fixture.Create<FlexibleConfigEngine.CLI.Gather>();

            A.CallTo(() => filesystem.FileExist("config.csx")).Returns(true);
            A.CallTo(() => session.GatherOnly("config.csx", A<string>.Ignored)).Throws<Exception>();


            //Act
            sut.ProcessCommand(new string[] { });

            //Assert
            A.CallTo(() => console.Error(A<string>.Ignored, A<string>.Ignored)).WhenArgumentsMatch(args =>
                    args[0].ToString().Contains("There was a problem running apply! Error Details: {error}"))
                .MustHaveHappened();
        }
    }
}
