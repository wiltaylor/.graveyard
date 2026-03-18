using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using FluentTest.UnitTest.TestObjects;
using Ploeh.AutoFixture;
using Ploeh.AutoFixture.AutoFakeItEasy;
using Xunit;

namespace FluentTest.UnitTest.Examples
{
    public class BasicSamples
    {
        [Fact]
        public void TyicalExampleTest()
        {
            var container = new Fixture().Customize(new AutoFakeItEasyCustomization());
            var sut = container.Create<TestObject>();

            sut.AProperty = "Example";

            Assert.True(sut.AProperty == "Example");
        }

        [Fact]
        public void TestImplimentedWithFluentTest() => new Test().CreateWithSut<TestObject>()
            .Arrange(c => c.Sut = c.Container.Create<TestObject>())
            .Act(c => c.Sut.AProperty = "Example")
            .Assert(c => c.Sut.AProperty == "Example");

        [Fact]
        public void TestExceptionHandler() => new Test().CreateWithSut<TestObject>()
            .Arrange(c => c.Sut = c.Container.Create<TestObject>())
            .ActAndAssertThrows<Exception>(c => { throw new Exception("Example Exception"); });

    }
}
