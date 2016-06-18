using NUnit.Framework;
using System.Linq;
using FakeItEasy;
using VMLib.IOC;

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

        public IServiceDiscovery DefaultServiceDiscovery()
        {
            var srv = A.Fake<IServiceDiscovery>();
            ServiceDiscovery.UnitTestInjection(srv);
            return srv;
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
            var srv = DefaultServiceDiscovery();

            var sut = DefaultHyperVHypervisorInfoFactory();

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
