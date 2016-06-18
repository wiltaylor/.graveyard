using NUnit.Framework;
using System.Linq;

namespace VMLib.HyperV.UnitTest
{
    [TestFixture]
    public class HyperVHypervisorInfoTests
    {
        public IHypervisorInfo DefaultHyperVHypervisorInfoFactory()
        {
            var sut = new HyperVHypervisorInfo();

            return sut;
        }

        [Test]
        public void Name_CheckNameProperty_IsHyperV()
        {
            var sut = DefaultHyperVHypervisorInfoFactory();

            Assert.That(sut.Name == "HyperV");
        }

        [TestCase("Host")]
        public void CreateConnectionInfo_CheckForExpectedProperties_ContainsExpectedPRoperties(string property)
        {
            var sut = DefaultHyperVHypervisorInfoFactory();

            var result = sut.CreateConnectionInfo();

            Assert.That(result.Properties.Keys.Contains(property));
        }

        [Test]
        public void CreateConnectionInfo_DefaultHostName_IsLocalHost()
        {

            var sut = DefaultHyperVHypervisorInfoFactory();

            var result = sut.CreateConnectionInfo();

            Assert.That(result.Properties["Host"] == "localhost");
        }
    }
}
