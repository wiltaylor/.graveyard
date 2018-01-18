using System;
using System.Collections.Generic;
using System.Linq;
using AutoFixture;
using FakeItEasy;
using FlexibleConfigEngine.Core.Config;
using FlexibleConfigEngine.Core.Exceptions;
using FlexibleConfigEngine.Core.Gather;
using FlexibleConfigEngine.Core.Graph;
using FlexibleConfigEngine.Core.IOC;
using FlexibleConfigEngine.Core.Resource;
using Xunit;

namespace FlexibleConfigUnitTest.Config
{
    public class ConfigManagerTests
    {
        [Fact]
        public void When_CallingRun_Should_StoreSettingsInDataStore()
        {
            //Arrange
            var fixture = Test.Fixture;
            var datastore = fixture.Freeze<IDataStore>();
            var settings = fixture.Create<Dictionary<string, string>>();
            var sut = fixture.Create<ConfigManager>();

            //Act
            sut.BuildRunList(settings);

            //Assert
            A.CallTo(() => datastore.Write("ConfigSettings", settings, false)).MustHaveHappened();
        }

        [Fact]
        public void When_CallingRunWithNullSettings_Should_NotCallDataStore()
        {
            //Arrange
            var fixture = Test.Fixture;
            var datastore = fixture.Freeze<IDataStore>();
            var sut = fixture.Create<ConfigManager>();

            //Act
            sut.BuildRunList(null);

            //Assert
            A.CallTo(() => datastore.Write("ConfigSettings", A<Dictionary<string,string>>.Ignored, false)).MustNotHaveHappened();
        }

        [Fact]
        public void When_CallingRunConfigItemThatReturnsTrueFromCriteria_Should_beReturned()
        {
            //Arrange
            var fixture = Test.Fixture;
            var graphManager = fixture.Freeze<IGraphManager>();
            var config = fixture.Create<ConfigItem>();
            var sut = fixture.Create<ConfigManager>();

            config.Criteria.Clear();
            config.Criteria.Add(data => true);
            config.Dependancy = null;

            A.CallTo(() => graphManager.Config).Returns(new[] {config});

            //Act
            var result = sut.BuildRunList(null);

            //Assert
            Assert.Contains(config, result);

        }

        [Fact]
        public void When_CallingRunConfigItemThatReturnsFalseFromCriteria_Should_NotBeReturned()
        {
            //Arrange
            var fixture = Test.Fixture;
            var graphManager = fixture.Freeze<IGraphManager>();
            var config = fixture.Create<ConfigItem>();
            var sut = fixture.Create<ConfigManager>();

            config.Criteria.Clear();
            config.Criteria.Add(data => false);
            config.Dependancy = null;

            A.CallTo(() => graphManager.Config).Returns(new[] { config });

            //Act
            var result = sut.BuildRunList(null);

            //Assert
            Assert.DoesNotContain(config, result);
        }

        [Fact]
        public void When_PassingInMultipleConfigItems_Should_BeReturnedInThatOrders()
        {
            //Arrange
            var fixture = Test.Fixture;
            var graphManager = fixture.Freeze<IGraphManager>();

            var config1 = fixture.Create<ConfigItem>();
            var config2 = fixture.Create<ConfigItem>();
            var config3 = fixture.Create<ConfigItem>();

            config1.Criteria.Clear();
            config2.Criteria.Clear();
            config3.Criteria.Clear();

            config1.Criteria.Add(d => true);
            config2.Criteria.Add(d => true);
            config3.Criteria.Add(d => true);

            config1.Dependancy = null;
            config2.Dependancy = null;
            config3.Dependancy = null;

            A.CallTo(() => graphManager.Config).Returns(new[] {config1, config2, config3});

            var sut = fixture.Create<ConfigManager>();


            //Act
            var result = sut.BuildRunList(null).ToArray();


            //Assert
            Assert.Same(config1, result[0]);
            Assert.Same(config2, result[1]);
            Assert.Same(config3, result[2]);
        }

        [Fact]
        public void When_PassingInMultipleConfigItemsWithDependancy_Should_ReturnInDependancyOrder()
        {
            //Arrange
            var fixture = Test.Fixture;
            var graphManager = fixture.Freeze<IGraphManager>();

            var config1 = fixture.Create<ConfigItem>();
            var config2 = fixture.Create<ConfigItem>();
            var config3 = fixture.Create<ConfigItem>();

            config1.Criteria.Clear();
            config2.Criteria.Clear();
            config3.Criteria.Clear();

            config1.Criteria.Add(d => true);
            config2.Criteria.Add(d => true);
            config3.Criteria.Add(d => true);

            config1.Dependancy = config3.Name;
            config2.Dependancy = config1.Name;
            config3.Dependancy = null;

            A.CallTo(() => graphManager.Config).Returns(new[] { config1, config2, config3 });


            var sut = fixture.Create<ConfigManager>();


            //Act
            var result = sut.BuildRunList(null).ToArray();


            //Assert
            Assert.Same(config3, result[0]);
            Assert.Same(config1, result[1]);
            Assert.Same(config2, result[2]);

        }

        [Fact]
        public void When_PassingInDependancyThatDoesntExist_Should_Throw()
        {
            //Arrange
            var fixture = Test.Fixture;
            var graphManager = fixture.Freeze<IGraphManager>();
            var config = fixture.Create<ConfigItem>();
            var sut = fixture.Create<ConfigManager>();

            config.Criteria.Clear();
            config.Criteria.Add(data => true);
            config.Dependancy = "nonexisting";

            A.CallTo(() => graphManager.Config).Returns(new[] { config });

            //Act & Assert
            Assert.Throws<MissingDependancyException>(() => sut.BuildRunList(null));

        }

        [Fact]
        public void When_PassingInCircularDependancies_Should_Throw()
        {
            //Arrange
            var fixture = Test.Fixture;
            var graphManager = fixture.Freeze<IGraphManager>();
            var config1 = fixture.Create<ConfigItem>();
            var config2 = fixture.Create<ConfigItem>();
            var sut = fixture.Create<ConfigManager>();

            config1.Criteria.Clear();
            config1.Criteria.Add(data => true);
            config1.Dependancy = config2.Name;

            config2.Criteria.Clear();
            config2.Criteria.Add(data => true);
            config2.Dependancy = config1.Name;

            A.CallTo(() => graphManager.Config).Returns(new[] { config1, config2 });

            //Act & Assert
            Assert.Throws<DeepOrCirtularDependancyException>(() => sut.BuildRunList(null));
        }

        [Fact]
        public void When_PassingSelfAsReference_Should_Throw()
        {
            //Arrange
            var fixture = Test.Fixture;
            var graphManager = fixture.Freeze<IGraphManager>();
            var config = fixture.Create<ConfigItem>();
            var sut = fixture.Create<ConfigManager>();

            config.Criteria.Clear();
            config.Criteria.Add(data => true);
            config.Dependancy = config.Name;

            A.CallTo(() => graphManager.Config).Returns(new[] { config });

            //Act & Assert
            Assert.Throws<SelfReferenceException>(() => sut.BuildRunList(null));
        }

        [Fact]
        public void When_CallingApplyRnList_Should_CallTestOnEachItem()
        {
            //Arrange
            var fixture = Test.Fixture;
            var runList = fixture.CreateMany<ConfigItem>().ToArray();
            var drivers = new[] {A.Fake<IResourceDriver>(), A.Fake<IResourceDriver>(), A.Fake<IResourceDriver>()};
            var pluginMan = fixture.Freeze<IPluginManager>();
            var sut = fixture.Create<ConfigManager>();

            A.CallTo(() => pluginMan.GetResource(runList[0].Resource)).Returns(drivers[0]);
            A.CallTo(() => pluginMan.GetResource(runList[1].Resource)).Returns(drivers[1]);
            A.CallTo(() => pluginMan.GetResource(runList[2].Resource)).Returns(drivers[2]);

            //Act
            sut.ApplyRunList(runList);

            //Assert
            A.CallTo(() => drivers[0].Test()).MustHaveHappened()
                .Then(A.CallTo(() => drivers[1].Test()).MustHaveHappened())
                .Then(A.CallTo(() => drivers[2].Test()).MustHaveHappened());

        }

        [Fact]
        public void When_CallingApplyRunListWithSomeItemsConfigured_should_OnlyCallApplyOnUnconfiguredItems()
        {
            //Arrange
            var fixture = Test.Fixture;
            var runList = fixture.CreateMany<ConfigItem>().ToArray();
            var drivers = new[] { A.Fake<IResourceDriver>(), A.Fake<IResourceDriver>(), A.Fake<IResourceDriver>() };
            var pluginMan = fixture.Freeze<IPluginManager>();
            var sut = fixture.Create<ConfigManager>();

            A.CallTo(() => pluginMan.GetResource(runList[0].Resource)).Returns(drivers[0]);
            A.CallTo(() => pluginMan.GetResource(runList[1].Resource)).Returns(drivers[1]);
            A.CallTo(() => pluginMan.GetResource(runList[2].Resource)).Returns(drivers[2]);

            A.CallTo(() => drivers[0].Test()).Returns(ResourceState.Configured);
            A.CallTo(() => drivers[1].Test()).Returns(ResourceState.NotConfigured);
            A.CallTo(() => drivers[2].Test()).Returns(ResourceState.Configured);

            //Act
            sut.ApplyRunList(runList);

            //Assert
            A.CallTo(() => drivers[0].Apply()).MustNotHaveHappened();
            A.CallTo(() => drivers[1].Apply()).MustHaveHappened();
            A.CallTo(() => drivers[2].Apply()).MustNotHaveHappened();

        }

        [Fact]
        public void When_CallingApplyAndAnItemReturnsNeedsReboot_Should_ReturnNeedsRebootAndNotRunAnyMoreConfig()
        {
            //Arrange
            var fixture = Test.Fixture;
            var runList = fixture.CreateMany<ConfigItem>().ToArray();
            var drivers = new[] { A.Fake<IResourceDriver>(), A.Fake<IResourceDriver>(), A.Fake<IResourceDriver>() };
            var pluginMan = fixture.Freeze<IPluginManager>();
            var sut = fixture.Create<ConfigManager>();

            A.CallTo(() => pluginMan.GetResource(runList[0].Resource)).Returns(drivers[0]);
            A.CallTo(() => pluginMan.GetResource(runList[1].Resource)).Returns(drivers[1]);
            A.CallTo(() => pluginMan.GetResource(runList[2].Resource)).Returns(drivers[2]);

            A.CallTo(() => drivers[0].Test()).Returns(ResourceState.NotConfigured);
            A.CallTo(() => drivers[1].Test()).Returns(ResourceState.NotConfigured);
            A.CallTo(() => drivers[2].Test()).Returns(ResourceState.NotConfigured);

            A.CallTo(() => drivers[0].Apply()).Returns(ResourceState.NeedReboot);
            A.CallTo(() => drivers[1].Apply()).Returns(ResourceState.Configured);
            A.CallTo(() => drivers[2].Apply()).Returns(ResourceState.Configured);

            //Act
            var result = sut.ApplyRunList(runList);

            //Assert
            Assert.Contains(result, r => r.State == ResourceState.NeedReboot);
            A.CallTo(() => drivers[0].Apply()).MustHaveHappened();
            A.CallTo(() => drivers[1].Apply()).MustNotHaveHappened();
        }

        [Fact]
        public void When_CallingApplyAndAnItemReturnsNeedRebootOnTest_Should_Throw()
        {
            //Arrange
            var fixture = Test.Fixture;
            var runList = fixture.CreateMany<ConfigItem>().ToArray();
            var drivers = new[] { A.Fake<IResourceDriver>(), A.Fake<IResourceDriver>(), A.Fake<IResourceDriver>() };
            var pluginMan = fixture.Freeze<IPluginManager>();
            var sut = fixture.Create<ConfigManager>();

            A.CallTo(() => pluginMan.GetResource(runList[0].Resource)).Returns(drivers[0]);

            A.CallTo(() => drivers[0].Test()).Returns(ResourceState.NeedReboot);

            //Act & Assert
            Assert.Throws<InvalidResourceState>(() => 
                sut.ApplyRunList(runList)
            );
        }

        [Fact]
        public void When_CallingApplyAndItemIsUncofigured_Should_CallTestAgainIfWasSuccesful()
        {
            //Arrange
            var fixture = Test.Fixture;
            var runList = fixture.CreateMany<ConfigItem>().ToArray();
            var drivers = new[] { A.Fake<IResourceDriver>(), A.Fake<IResourceDriver>(), A.Fake<IResourceDriver>() };
            var pluginMan = fixture.Freeze<IPluginManager>();
            var sut = fixture.Create<ConfigManager>();

            A.CallTo(() => pluginMan.GetResource(runList[0].Resource)).Returns(drivers[0]);
            A.CallTo(() => drivers[0].Test()).Returns(ResourceState.NotConfigured).Once()
                .Then.Returns(ResourceState.Configured).Once();

            //Act
            var result = sut.ApplyRunList(runList);

            //Assert
            A.CallTo(() => drivers[0].Test()).MustHaveHappened(Repeated.Like(r => r ==2));
        }

        [Fact]
        public void When_CallingTest_Should_TestAllItems()
        {
            //Arrange
            var fixture = Test.Fixture;
            var runList = fixture.CreateMany<ConfigItem>().ToArray();
            var drivers = new[] { A.Fake<IResourceDriver>(), A.Fake<IResourceDriver>(), A.Fake<IResourceDriver>() };
            var pluginMan = fixture.Freeze<IPluginManager>();
            var sut = fixture.Create<ConfigManager>();

            A.CallTo(() => pluginMan.GetResource(runList[0].Resource)).Returns(drivers[0]);
            A.CallTo(() => pluginMan.GetResource(runList[1].Resource)).Returns(drivers[1]);
            A.CallTo(() => pluginMan.GetResource(runList[2].Resource)).Returns(drivers[2]);

            A.CallTo(() => drivers[0].Test()).Returns(ResourceState.NotConfigured);
            A.CallTo(() => drivers[1].Test()).Returns(ResourceState.NotConfigured);
            A.CallTo(() => drivers[2].Test()).Returns(ResourceState.NotConfigured);

            //Act
            sut.Test(runList);

            //Assert
            A.CallTo(() => drivers[0].Test()).MustHaveHappened();
            A.CallTo(() => drivers[1].Test()).MustHaveHappened();
            A.CallTo(() => drivers[2].Test()).MustHaveHappened();

        }

        [Fact]
        public void When_CallingTest_Should_ReturnStatesOfTestsAsResults()
        {
            //Arrange
            var fixture = Test.Fixture;
            var runList = fixture.CreateMany<ConfigItem>().ToArray();
            var drivers = new[] { A.Fake<IResourceDriver>(), A.Fake<IResourceDriver>(), A.Fake<IResourceDriver>() };
            var pluginMan = fixture.Freeze<IPluginManager>();
            var sut = fixture.Create<ConfigManager>();

            A.CallTo(() => pluginMan.GetResource(runList[0].Resource)).Returns(drivers[0]);
            A.CallTo(() => pluginMan.GetResource(runList[1].Resource)).Returns(drivers[1]);
            A.CallTo(() => pluginMan.GetResource(runList[2].Resource)).Returns(drivers[2]);

            A.CallTo(() => drivers[0].Test()).Returns(ResourceState.Configured);
            A.CallTo(() => drivers[1].Test()).Returns(ResourceState.Configured);
            A.CallTo(() => drivers[2].Test()).Returns(ResourceState.NotConfigured);

            //Act
            var result = sut.Test(runList).ToArray();

            //Assert
            Assert.Equal(ResourceState.Configured, result[0].State);
            Assert.Equal(ResourceState.Configured, result[1].State);
            Assert.Equal(ResourceState.NotConfigured, result[2].State);

        }
    }
}
