using System;
using System.Collections.Generic;
using System.Linq;
using AutoFixture;
using EasyRoslynScript;
using FakeItEasy;
using FlexibleConfigEngine.Core.Config;
using FlexibleConfigEngine.Core.ConfigSession;
using FlexibleConfigEngine.Core.Exceptions;
using FlexibleConfigEngine.Core.Gather;
using FlexibleConfigEngine.Core.Graph;
using FlexibleConfigEngine.Core.Helper;
using FlexibleConfigEngine.Core.Resource;
using Newtonsoft.Json;
using Xunit;

namespace FlexibleConfigUnitTest.ConfigSession
{
    public class SessionTest 
    {
        [Fact]
        public void When_CallingValidate_Should_CallScriptRunner()
        {
            //Arrange
            var fixture = Test.Fixture;
            var runner = fixture.Freeze<IScriptRunner>();
            var sut = fixture.Create<Session>();

            //Act
            sut.Validate("config.csx");

            //Assert
            A.CallTo(() => runner.ExecuteFile("config.csx")).MustHaveHappened();

        }

        [Fact]
        public void When_CallingValidateAndExceptionIsThrown_Should_ReportErrorToConsole()
        {
            //Arrange
            var fixture = Test.Fixture;
            var runner = fixture.Freeze<IScriptRunner>();
            var console = fixture.Freeze<IConsole>();
            var sut = fixture.Create<Session>();

            A.CallTo(() => runner.ExecuteFile(A<string>.Ignored)).Throws<Exception>();

            //Act
            sut.Validate("config.csx");

            //Assert
            A.CallTo(() => console.Error("There was an error while running script! Error: {error}", A<object>.Ignored))
                .MustHaveHappened();
        }

        [Fact]
        public void When_CallingValidateAndScriptDoesntThrow_Should_CallGraphValidate()
        {
            //Arrange
            var fixture = Test.Fixture;
            var graphManager = fixture.Freeze<IGraphManager>();
            var sut = fixture.Create<Session>();

            //Act
            sut.Validate("config.csx");

            //Assert
            A.CallTo(() => graphManager.Validate()).MustHaveHappened();
        }

        [Fact]
        public void When_CallingValidateAndGraphValidateThrows_Should_ReportErrorToConsole()
        {
            //Arrange
            var fixture = Test.Fixture;
            var graphManager = fixture.Freeze<IGraphManager>();
            var console = fixture.Freeze<IConsole>();
            var sut = fixture.Create<Session>();

            A.CallTo(() => graphManager.Validate()).Throws(new GraphValidationException("Failed validation message"));

            //Act
            sut.Validate("config.csx");

            A.CallTo(() => console.Error("Validation Error! Error: {details}", A<object>.Ignored))
                .MustHaveHappened();

        }

        [Fact]
        public void When_CallingGather_Should_CallScriptRunner()
        {
            //Arrange
            var fixture = Test.Fixture;
            var runner = fixture.Freeze<IScriptRunner>();
            var sut = fixture.Create<Session>();

            //Act
            sut.GatherOnly("config.csx", "gather.json");

            //Assert
            A.CallTo(() => runner.ExecuteFile("config.csx")).MustHaveHappened();
        }

        [Fact]
        public void When_CallingGatherAndExceptionIsThrown_Should_ReportErrorToConsole()
        {
            //Arrange
            var fixture = Test.Fixture;
            var runner = fixture.Freeze<IScriptRunner>();
            var console = fixture.Freeze<IConsole>();
            var sut = fixture.Create<Session>();

            A.CallTo(() => runner.ExecuteFile(A<string>.Ignored)).Throws<Exception>();

            //Act
            sut.GatherOnly("config.csx", "gather.json");

            //Assert
            A.CallTo(() => console.Error("There was an error while running script! Error: {error}", A<object>.Ignored))
                .MustHaveHappened();
        }

        [Fact]
        public void When_CallingGatherAndItsValid_Should_CallGatherManagerToRun()
        {
            //Arrange
            var fixture = Test.Fixture;
            var gatherManager = fixture.Freeze<IGatherManager>();
            var sut = fixture.Create<Session>();

            //Act
            sut.GatherOnly("config.csx", "gather.json");

            //Assert
            A.CallTo(() => gatherManager.Run()).MustHaveHappened();
        }

        [Fact]
        public void When_CallingGatherAndGatherManagerThrows_Should_ReportErrorToConsole()
        {
            //Arrange
            var fixture = Test.Fixture;
            var gatherManager = fixture.Freeze<IGatherManager>();
            var console = fixture.Freeze<IConsole>();

            var sut = fixture.Create<Session>();

            A.CallTo(() => gatherManager.Run()).Throws<Exception>();

            //Act
            sut.GatherOnly("config.csx", "gather.json");

            //Assert
            A.CallTo(() => console.Error("There was an error while running gathers! Error: {error}", A<object>.Ignored))
                .MustHaveHappened();
        }

        [Fact]
        public void When_CallingGatherWithoutError_Should_StoreGatherDataInFile()
        {
            //Arrange
            var fixture = Test.Fixture;
            var datamanager = fixture.Freeze<IDataStore>();
            var filesys = fixture.Freeze<IFileSystem>();
            var sut = fixture.Create<Session>();


            A.CallTo(() => datamanager.GetPersistString(true)).Returns("{json data}");


            //Act
            sut.GatherOnly("config.csx", "gather.json");

            //Assert
            A.CallTo(() => filesys.WriteFile("gather.json", "{json data}")).MustHaveHappened();

        }

        [Fact]
        public void When_CallingGatherAndSavingFileThrows_Should_ReportErrorToConsole()
        {
            //Arrange
            var fixture = Test.Fixture;
            var datamanager = fixture.Freeze<IDataStore>();
            var filesys = fixture.Freeze<IFileSystem>();
            var console = fixture.Freeze<IConsole>();
            var sut = fixture.Create<Session>();


            A.CallTo(() => datamanager.GetPersistString(true)).Returns("{json data}");
            A.CallTo(() => filesys.WriteFile(A<string>.Ignored, A<string>.Ignored)).Throws<Exception>();


            //Act
            sut.GatherOnly("config.csx", "gather.json");

            //Assert
            A.CallTo(() => console.Error("There was an error while writing gather data to disk! Error: {error}", A<object>.Ignored))
                .MustHaveHappened();
        }

        [Fact]
        public void When_CallingApplyAndItsValid_Should_CallGatherManagerToRun()
        {
            //Arrange
            var fixture = Test.Fixture;
            var gatherManager = fixture.Freeze<IGatherManager>();
            var sut = fixture.Create<Session>();

            //Act
            sut.Apply("config.csx", null);

            //Assert
            A.CallTo(() => gatherManager.Run()).MustHaveHappened();
        }

        [Fact]
        public void When_CallingApplyAndItsNotValid_Should_NotCallGatherManager()
        {
            //Arrange
            var fixture = Test.Fixture;
            var runner = fixture.Freeze<IScriptRunner>();
            var gatherManager = fixture.Freeze<IGatherManager>();
            var sut = fixture.Create<Session>();

            A.CallTo(() => runner.ExecuteFile(A<string>.Ignored)).Throws<Exception>();

            //Act
            sut.Apply("config.csx", null);

            //Assert
            A.CallTo(() => gatherManager.Run()).MustNotHaveHappened();
        }

        [Fact]
        public void When_CallingApplyAndGatherManagerThrows_Should_ReportErrorToConsole()
        {
            //Arrange
            var fixture = Test.Fixture;
            var gatherManager = fixture.Freeze<IGatherManager>();
            var console = fixture.Freeze<IConsole>();

            var sut = fixture.Create<Session>();

            A.CallTo(() => gatherManager.Run()).Throws<Exception>();

            //Act
            sut.Apply("config.csx", null);

            //Assert
            A.CallTo(() => console.Error("There was an error while running gathers! Error: {error}", A<object>.Ignored))
                .MustHaveHappened();
        }

        [Fact]
        public void When_CallingApplyWithNoErrors_Should_CallRunOnConfigManager()
        {
            //Arrange
            var fixture = Test.Fixture;
            var configManager = fixture.Freeze<IConfigManager>();
            var sut = fixture.Create<Session>();

            //Act
            sut.Apply("config.csx", null);

            //Assert
            A.CallTo(() => configManager.BuildRunList(null)).MustHaveHappened();
        }

        [Fact]
        public void When_CallingApplyWithMissingSettingFile_Should_ReportErrorToConsole()
        {
            //Arrange
            var fixture = Test.Fixture;
            var fileSystem = fixture.Freeze<IFileSystem>();
            var console = fixture.Freeze<IConsole>();
            var sut = fixture.Create<Session>();

            A.CallTo(() => fileSystem.FileExist("settings.json")).Returns(false);

            //Act
            sut.Apply("config.csx", "settings.json");

            //Assert
            A.CallTo(() => console.Error("Unable to find settings file {file}", A<object>.Ignored))
                .MustHaveHappened();
        }

        [Fact]
        public void When_CallingApplyWithSettingsFile_Should_BeDeserialisedFromJsonToDictionary()
        {
            //Arrange
            var fixture = Test.Fixture;
            var fileSystem = fixture.Freeze<IFileSystem>();
            var configManager = fixture.Freeze<IConfigManager>();
            var sut = fixture.Create<Session>();

            A.CallTo(() => fileSystem.FileExist("settings.json")).Returns(true);
            A.CallTo(() => fileSystem.ReadFile("settings.json")).Returns("{ \"Item\": \"test\", \"Item2\": \"Test2\" }");

            //Act
            sut.Apply("config.csx", "settings.json");

            //Assert
            A.CallTo(() => configManager.BuildRunList(A<Dictionary<string, string>>.Ignored))
                .WhenArgumentsMatch(args => args[0] != null)
                .MustHaveHappened();
        }

        [Fact]
        public void When_CallingApplyWithSettingsFileButAFileExceptionIsRaised_Should_ReportErrorToConsole()
        {
            //Arrange
            var fixture = Test.Fixture;
            var fileSystem = fixture.Freeze<IFileSystem>();
            var console = fixture.Freeze<IConsole>();

            var sut = fixture.Create<Session>();

            A.CallTo(() => fileSystem.FileExist("settings.json")).Returns(true);
            A.CallTo(() => fileSystem.ReadFile("settings.json")).Throws<Exception>();

            //Act
            sut.Apply("config.csx", "settings.json");

            //Assert
            A.CallTo(() => console.Error("There was an error while trying to read {file} Error: {error}", A<object>.Ignored, A<object>.Ignored))
                .MustHaveHappened();
        }

        [Fact]
        public void When_CallingApplyRunList_Should_BePassedToApplyOnConfigManager()
        {
            //Arrange
            var fixture = Test.Fixture;
            var fileSystem = fixture.Freeze<IFileSystem>();
            var runList = fixture.CreateMany<ConfigItem>();
            var configMan = fixture.Freeze<IConfigManager>();
            var sut = fixture.Create<Session>();

            A.CallTo(() => fileSystem.FileExist("settings.json")).Returns(true);
            A.CallTo(() => configMan.BuildRunList(A<Dictionary<string, string>>.Ignored)).Returns(runList);


            //Act
            sut.Apply("config.csx", null);

            //Assert
            A.CallTo(() => configMan.ApplyRunList(runList)).MustHaveHappened();
        }

        [Fact]
        public void When_CallingApplyAndResultHasRebootInIt_Should_SetExitCodeToReboot()
        {
            //Arrange
            var fixture = Test.Fixture;
            var fileSystem = fixture.Freeze<IFileSystem>();
            var runList = fixture.CreateMany<ConfigItem>();
            var configMan = fixture.Freeze<IConfigManager>();
            var configResult = fixture.Create<ConfigurationResult>();
            var environment = fixture.Freeze<IEnvironmentHelper>();
            var sut = fixture.Create<Session>();

            configResult.State = ResourceState.NeedReboot;

            A.CallTo(() => fileSystem.FileExist("settings.json")).Returns(true);
            A.CallTo(() => configMan.BuildRunList(A<Dictionary<string, string>>.Ignored)).Returns(runList);
            A.CallTo(() => configMan.ApplyRunList(A<IEnumerable<ConfigItem>>.Ignored)).Returns(new[] {configResult});


            //Act
            sut.Apply("config.csx", null);

            //Assert
            A.CallTo(() => environment.SetExitCode(ExitCodes.Reboot)).MustHaveHappened();
        }

        [Fact]
        public void When_CallingApplyAndResultHasNotConfiguredInIt_Should_SetExitCodeToError()
        {
            //Arrange
            var fixture = Test.Fixture;
            var fileSystem = fixture.Freeze<IFileSystem>();
            var runList = fixture.CreateMany<ConfigItem>();
            var configMan = fixture.Freeze<IConfigManager>();
            var configResult = fixture.Create<ConfigurationResult>();
            var environment = fixture.Freeze<IEnvironmentHelper>();
            var sut = fixture.Create<Session>();

            configResult.State = ResourceState.NotConfigured;

            A.CallTo(() => fileSystem.FileExist("settings.json")).Returns(true);
            A.CallTo(() => configMan.BuildRunList(A<Dictionary<string, string>>.Ignored)).Returns(runList);
            A.CallTo(() => configMan.ApplyRunList(A<IEnumerable<ConfigItem>>.Ignored)).Returns(new[] { configResult });


            //Act
            sut.Apply("config.csx", null);

            //Assert
            A.CallTo(() => environment.SetExitCode(ExitCodes.Error)).MustHaveHappened();
        }

        [Fact]
        public void When_CallingApplyAndResultOnlyHasConfiguredInIt_Should_SetExitCodeToOk()
        {
            //Arrange
            var fixture = Test.Fixture;
            var fileSystem = fixture.Freeze<IFileSystem>();
            var runList = fixture.CreateMany<ConfigItem>();
            var configMan = fixture.Freeze<IConfigManager>();
            var configResult = fixture.Create<ConfigurationResult>();
            var environment = fixture.Freeze<IEnvironmentHelper>();
            var sut = fixture.Create<Session>();

            configResult.State = ResourceState.Configured;

            A.CallTo(() => fileSystem.FileExist("settings.json")).Returns(true);
            A.CallTo(() => configMan.BuildRunList(A<Dictionary<string, string>>.Ignored)).Returns(runList);
            A.CallTo(() => configMan.ApplyRunList(A<IEnumerable<ConfigItem>>.Ignored)).Returns(new[] { configResult });


            //Act
            sut.Apply("config.csx", null);

            //Assert
            A.CallTo(() => environment.SetExitCode(ExitCodes.Ok)).MustHaveHappened();
        }

        [Fact]
        public void When_CallingApplyWithTestSetToTrue_Should_CallTestInsteadOfApply()
        {
            //Arrange
            var fixture = Test.Fixture;
            var runList = fixture.CreateMany<ConfigItem>();
            var configMan = fixture.Freeze<IConfigManager>();
            var sut = fixture.Create<Session>();

            A.CallTo(() => configMan.BuildRunList(A<Dictionary<string, string>>.Ignored)).Returns(runList);

            //Act
            sut.Apply("config.csx", null, true);

            //Assert
            A.CallTo(() => configMan.Test(runList)).MustHaveHappened();
            A.CallTo(() => configMan.ApplyRunList(runList)).MustNotHaveHappened();
        }

        [Fact]
        public void When_CallingApplyIfDataJSONExists_Should_BeLoadedIntoDataStore()
        {
            //Arrange
            var fixture = Test.Fixture;
            var datamanager = fixture.Freeze<IDataStore>();
            var fileSystem = fixture.Freeze<IFileSystem>();
            var jsonData = fixture.Create<string>();
            var sut = fixture.Create<Session>();

            A.CallTo(() => datamanager.GetPersistString(false)).Returns(jsonData);

            //Act
            sut.Apply("config.csx", null);

            //Assert
            A.CallTo(() => datamanager.GetPersistString(false)).MustHaveHappened();
            A.CallTo(() => fileSystem.WriteFile("data.json", jsonData)).MustHaveHappened();

        }

        [Fact]
        public void When_CallingApplyIfDataJSONExistsIT_Should_BeLoaded()
        {
            //Arrange
            var fixture = Test.Fixture;
            var datamanager = fixture.Freeze<IDataStore>();
            var fileSystem = fixture.Freeze<IFileSystem>();
            var sut = fixture.Create<Session>();

            var dat = fixture.Create<Dictionary<string, Dictionary<string, string>>>();
            var json = JsonConvert.SerializeObject(dat);
            var firstkey = dat.Keys.First();

            A.CallTo(() => fileSystem.FileExist("data.json")).Returns(true);
            A.CallTo(() => fileSystem.ReadFile("data.json")).Returns(json);

            //Act
            sut.Apply("config.csx", null);


            //Assert
            A.CallTo(() => datamanager.Write(firstkey, A<Dictionary<string, string>>.Ignored, true)).MustHaveHappened();
        }

        [Fact]
        public void When_CallingApply_Should_StoreResultsInResultsJson()
        {
            //Arrange
            var fixture = Test.Fixture;
            var fileSystem = fixture.Freeze<IFileSystem>();
            var sut = fixture.Create<Session>();

            //Act
            sut.Apply("config.csx", null);

            //Assert
            A.CallTo(() => fileSystem.WriteFile("result.json", A<string>.Ignored)).MustHaveHappened();

        }
    }
}
