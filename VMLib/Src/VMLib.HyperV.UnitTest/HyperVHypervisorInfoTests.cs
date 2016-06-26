using NUnit.Framework;
using System.Linq;
using FakeItEasy;
using VMLib.IOC;
using VMLib.UnitTest;

namespace VMLib.HyperV.UnitTest
{
    [TestFixture]
    public class HyperVHypervisorInfoTests
    {
        public IHypervisorInfo DefaultHyperVHypervisorInfoFactory(IServiceDiscovery srvDiscovery = null)
        {
            if (srvDiscovery == null)
                srvDiscovery = FakeServiceDiscovery.ReturnTestableInstance();

            var sut = new HyperVHypervisorInfo();

            return sut;
        }

        [Test]
        public void Name_CheckNameProperty_IsHyperV()
        {
            var sut = DefaultHyperVHypervisorInfoFactory();

            Assert.That(sut.Name == "HyperV");
        }

        [Test]
        public void Constructor_RegistersHyperVHypervisorDuringConstruction()
        {
            var srv = FakeServiceDiscovery.ReturnTestableInstance();

            var sut = DefaultHyperVHypervisorInfoFactory(srvDiscovery: srv);

            A.CallTo(() => srv.AddType<IHypervisor, HyperVHypervisor>(sut.Name)).MustHaveHappened();
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
