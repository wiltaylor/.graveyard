using System.Collections.Generic;
using System.Linq;
using AutoFixture;
using FakeItEasy;
using FlexibleConfigEngine.Core.Gather;
using FlexibleConfigEngine.Core.Graph;
using FlexibleConfigEngine.Core.IOC;
using Xunit;

namespace FlexibleConfigUnitTest.Gather
{
    public class GatherManagerTest
    {
        [Fact]
        public void When_RunIsCalled_Should_GetGatherersFromGraphAPI()
        {
            //Arrange
            var fixture = Test.Fixture;
            var graphManager = fixture.Freeze<IGraphManager>();
            var sut = fixture.Create<GatherManager>();

            //Act
            sut.Run();

            //Assert
            A.CallTo(() => graphManager.Gathers).MustHaveHappened();
        }

        [Fact]
        public void When_RunIsCalled_Should_GetGatherDriversForEachGatherer()
        {
            //Arrange
            var fixture = Test.Fixture;
            var graphManager = fixture.Freeze<IGraphManager>();
            var pluginMan = fixture.Freeze<IPluginManager>();
            var gatherers = fixture.CreateMany<FlexibleConfigEngine.Core.Graph.Gather>().ToArray();
            var sut = fixture.Create<GatherManager>();

            A.CallTo(() => graphManager.Gathers).Returns(gatherers);

            //Act
            sut.Run();

            //Assert
            A.CallTo(() => pluginMan.GetGather(gatherers.First().Name)).MustHaveHappened();
        }

        [Fact]
        public void When_RunIsCalled_Should_CallRunOnEachGatherer()
        {
            //Arrange
            var fixture = Test.Fixture;
            var graphManager = fixture.Freeze<IGraphManager>();
            var pluginMan = fixture.Freeze<IPluginManager>();
            var gatherers = new[] {fixture.Create<FlexibleConfigEngine.Core.Graph.Gather>()};
            var gatherdriver = fixture.Freeze<IGatherDriver>();
            var sut = fixture.Create<GatherManager>();

            A.CallTo(() => graphManager.Gathers).Returns(gatherers);
            A.CallTo(() => pluginMan.GetGather(A<string>.Ignored)).Returns(gatherdriver);

            //Act
            sut.Run();

            //Assert
            A.CallTo(() => gatherdriver.Run()).MustHaveHappened();
        }

        [Fact]
        public void When_RunIsCalled_Should_PassInProperties()
        {
            //Arrange
            var fixture = Test.Fixture;
            var graphManager = fixture.Freeze<IGraphManager>();
            var pluginMan = fixture.Freeze<IPluginManager>();
            var gatherers = new[] { fixture.Create<FlexibleConfigEngine.Core.Graph.Gather>() };
            var gatherdriver = fixture.Freeze<IGatherDriver>();
            var sut = fixture.Create<GatherManager>();

            A.CallTo(() => graphManager.Gathers).Returns(gatherers);
            A.CallTo(() => pluginMan.GetGather(A<string>.Ignored)).Returns(gatherdriver);

            //Act
            sut.Run();

            //Assert
            A.CallTo(() => gatherdriver.SetProperties(A<Dictionary<string,string>>.Ignored)).MustHaveHappened();
        }
    }
}
