using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using FakeItEasy;
using FluentTest.UnitTest.TestObjects;
using Ploeh.AutoFixture;
using Ploeh.AutoFixture.AutoFakeItEasy;
using Xunit;

namespace FluentTest.UnitTest
{
    public class DataTargetTests
    {
        [Fact]
        public void When_CallingDataWithAnEmptyDataSet_Should_NotCallTestPredicate()
        {
            var fixture = new Fixture().Customize(new AutoFakeItEasyCustomization());
            var config = A.Fake<TestConfigBase<IFakeContainer>>();
            fixture.Register(() => config);
            var sut = fixture.Create<DataTarget<IFakeContainer, object, object, string>>();
            var qty = 0;

            sut.Data(() => new List<string>(), c => qty++);

            Assert.Equal(0, qty);
        }

        [Theory]
        [InlineData(1)]
        [InlineData(10)]
        [InlineData(20)]
        public void When_CallingDataWith_Should_CallTestPredicateForEachItem(int items)
        {
            var fixture = new Fixture().Customize(new AutoFakeItEasyCustomization());
            var config = A.Fake<TestConfigBase<IFakeContainer>>();
            fixture.Register(() => config);
            var sut = fixture.Create<DataTarget<IFakeContainer, object, object, int>>();
            var qty = 0;
            var data = new List<int>();

            for(var i = 0; i < items; i++)
                data.Add(i);

            sut.Data(() => data, c => qty++);

            Assert.Equal(items, qty);
        }
    }
}
