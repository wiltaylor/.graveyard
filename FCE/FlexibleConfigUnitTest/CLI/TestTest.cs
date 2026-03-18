using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using AutoFixture;
using FakeItEasy;
using FlexibleConfigEngine.CLI;
using FlexibleConfigEngine.Core.ConfigSession;
using FlexibleConfigEngine.Core.Helper;
using Xunit;

namespace FlexibleConfigUnitTest.CLI
{
    public class TestTest
    {
        [Fact]
        public void When_RunningApplyAndNoConfigScript_Should_ReportError()
        {
            //Arrange
            var fixture = Test.Fixture;
            var filesystem = fixture.Freeze<IFileSystem>();
            var console = fixture.Freeze<IConsole>();
            var sut = fixture.Create<FlexibleConfigEngine.CLI.Test>();

            A.CallTo(() => filesystem.FileExist("config.csx")).Returns(false);


            //Act
            sut.ProcessCommand(new string[] { });

            //Assert
            A.CallTo(() => console.Error(A<string>.Ignored, A<object>.Ignored))
                .WhenArgumentsMatch(args => args[0].ToString().Contains("The configuration script {script} is missing!") && ((Object[])args[1]).First().ToString() == "config.csx")
                .MustHaveHappened();

        }

        [Fact]
        public void When_PassingInScriptSwitch_Should_CheckForNewScriptNameInstead()
        {
            //Arrange
            var fixture = Test.Fixture;
            var filesystem = fixture.Freeze<IFileSystem>();
            var console = fixture.Freeze<IConsole>();
            var sut = fixture.Create<FlexibleConfigEngine.CLI.Test>();

            A.CallTo(() => filesystem.FileExist("test.csx")).Returns(false);

            sut.AppendSwitch("script", "test.csx");


            //Act
            sut.ProcessCommand(new string[] { });

            //Assert
            A.CallTo(() => console.Error(A<string>.Ignored, A<string>.Ignored))
                .WhenArgumentsMatch(args => args[0].ToString().Contains("The configuration script {script} is missing!") && ((Object[])args[1]).First().ToString() == "test.csx")
                .MustHaveHappened();
        }

        [Fact]
        public void When_PassingInSettingsThatDoentExist_Should_ReportErrorToUse()
        {
            //Arrange
            var fixture = Test.Fixture;
            var filesystem = fixture.Freeze<IFileSystem>();
            var console = fixture.Freeze<IConsole>();
            var sut = fixture.Create<FlexibleConfigEngine.CLI.Test>();

            A.CallTo(() => filesystem.FileExist("config.csx")).Returns(true);

            sut.AppendSwitch("settings", "settings.json");

            //Act
            sut.ProcessCommand(new string[] { });

            //Assert
            A.CallTo(() => console.Error(A<string>.Ignored, A<string>.Ignored)).WhenArgumentsMatch(args =>
                    args[0].ToString().Contains("Can't find settings file {file}.") &&
                    ((Object[])args[1]).First().ToString().Contains("settings.json"))
                .MustHaveHappened();
        }

        [Fact]
        public void When_PassingInCorrectSettingsAndConfig_Should_CallSessionManager()
        {
            //Arrange
            var fixture = Test.Fixture;
            var filesystem = fixture.Freeze<IFileSystem>();
            var session = fixture.Freeze<ISession>();
            var sut = fixture.Create<FlexibleConfigEngine.CLI.Test>();

            A.CallTo(() => filesystem.FileExist("config.csx")).Returns(true);

            //Act
            sut.ProcessCommand(new string[] { });

            //Assert
            A.CallTo(() => session.Apply("config.csx", null, true)).MustHaveHappened();
        }

        [Fact]
        public void When_SessionThrowsAnException_Should_JustReportErrorToConsole()
        {
            //Arrange
            var fixture = Test.Fixture;
            var filesystem = fixture.Freeze<IFileSystem>();
            var session = fixture.Freeze<ISession>();
            var console = fixture.Freeze<IConsole>();
            var sut = fixture.Create<FlexibleConfigEngine.CLI.Test>();

            A.CallTo(() => filesystem.FileExist("config.csx")).Returns(true);
            A.CallTo(() => session.Apply(A<string>.Ignored, A<string>.Ignored, true)).Throws<Exception>();

            //Act
            sut.ProcessCommand(new string[] { });

            //Assert
            A.CallTo(() => console.Error(A<string>.Ignored, A<string>.Ignored)).WhenArgumentsMatch(args =>
                    args[0].ToString().Contains("There was a problem running apply! Error Details: {error}"))
                .MustHaveHappened();
        }
    }
}
