using System;
using System.Collections.Generic;
using System.Data;
using System.Linq;
using System.Text;
using AutoFixture;
using FlexibleConfigEngine.Core.Exceptions;
using FlexibleConfigEngine.Core.Graph;
using Xunit;

namespace FlexibleConfigUnitTest.Graph
{
    public class GraphManagerTests
    {
        [Fact]
        public void When_AddGatherIsCalled_Should_AddAGatherer()
        {
            //Arrange
            var fixture = Test.Fixture;
            var properties = fixture.Create<Dictionary<string, string>>();
            var sut = fixture.Create<GraphManager>();

            //Act
            sut.AddGather(new FlexibleConfigEngine.Core.Graph.Gather{ Name="Test", Properties = properties});

            //Assert
            Assert.Contains(sut.Gathers, g => g.Name == "Test");
        }

        [Fact]
        public void When_AddConfigIsCalled_Should_AddAConfigItem()
        {
            //Arrange
            var fixture = Test.Fixture;
            var properties = fixture.Create<Dictionary<string, string>>();
            var rows = fixture.Create<List<Dictionary<string, string>>>();
            var sut = fixture.Create<GraphManager>();

            //Act
            sut.AddConfig(new ConfigItem{Name = "Test", Resource = "TestResource", Dependancy = "TestDependancy", Properties = properties, RowData = rows});

            //Assert
            Assert.Contains(sut.Config, g => g.Name == "Test");
        }

        [Fact]
        public void When_TwoConfigsHaveTheSameName_Should_ThrowValidationException()
        {
            //Arrange
            var fixture = Test.Fixture;
            var sut = fixture.Create<GraphManager>();

            sut.AddConfig(new ConfigItem { Name = "Test"});
            sut.AddConfig(new ConfigItem { Name = "Test" });

            //Act & Assert
            Assert.Throws<GraphValidationException>(() => sut.Validate());
        }

        [Fact]
        public void When_ValidatingEmptyGraph_Should_NotThrow()
        {
            //Arrange
            var fixture = Test.Fixture;
            var sut = fixture.Create<GraphManager>();

            //Act
            sut.Validate();

            //Assert
            //No exceptions.

        }
    }
}
