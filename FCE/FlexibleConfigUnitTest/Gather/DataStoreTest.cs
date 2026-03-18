using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using AutoFixture;
using FlexibleConfigEngine.Core.Gather;
using Xunit;

namespace FlexibleConfigUnitTest.Gather
{
    public class DataStoreTest
    {
        [Fact]
        public void When_WriteIsCalled_Should_BeRetrivalble()
        {
            //Arrange
            var fixture = Test.Fixture;
            var data = fixture.Create<Dictionary<string, string>>();
            var sut = fixture.Create<DataStore>();

            //Act
            sut.Write("MyObject", data);

            //Asssert
            Assert.Equal(data, sut.Read("MyObject"));
        }

        [Fact]
        public void When_WriteIsCalledOnExistingObject_Should_Overwrite()
        {
            //Arrange
            var fixture = Test.Fixture;
            var data1 = fixture.Create<Dictionary<string, string>>();
            var data2 = fixture.Create<Dictionary<string, string>>();
            var sut = fixture.Create<DataStore>();

            //Act
            sut.Write("MyObject", data1);
            sut.Write("MyObject", data2);

            //Asssert
            Assert.Equal(data2, sut.Read("MyObject"));
        }

        [Fact]
        public void When_ReadIsCalledOnNonExistingObject_Should_ReturnNull()
        {
            //Arrange
            var fixture = Test.Fixture;
            var sut = fixture.Create<DataStore>();

            //Act
            //Do nothing.

            //Asssert
            Assert.Null(sut.Read("MYNonExistingValue"));
        }

        [Fact]
        public void When_SettingValueAsPersistant_Should_BeReturnedWhenSerialised()
        {
            //Arrange
            var fixture = Test.Fixture;
            var data = fixture.Create<Dictionary<string, string>>();
            var sut = fixture.Create<DataStore>();

            //Act
            sut.Write("TestData", data, true);

            //Assert
            Assert.Contains(data.Keys.First(), sut.GetPersistString());

        }


        [Fact]
        public void When_NotSettingValueasPersistant_Should_NotBeReturnedWhenSerialised()
        {
            //Arrange
            var fixture = Test.Fixture;
            var data = fixture.Create<Dictionary<string, string>>();
            var sut = fixture.Create<DataStore>();

            //Act
            sut.Write("TestData", data);

            //Assert
            Assert.DoesNotContain(data.Keys.First(), sut.GetPersistString());
        }
    }
}
