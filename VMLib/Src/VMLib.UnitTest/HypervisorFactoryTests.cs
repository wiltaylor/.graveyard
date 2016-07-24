using System.Linq;
using FakeItEasy;
using NUnit.Framework;
using VMLib.Exceptions;
using VMLib.IOC;

namespace VMLib.UnitTest
{
    [TestFixture]
    public class HypervisorFactoryTests
    {

        private IHypervisorFactory DefaultHypervisorFactory()
        {
            return new HypervisorFactory();
        }

        private IServiceDiscovery CreateFakeServiceDiscovery()
        {
            var sd = A.Fake<IServiceDiscovery>();
            ServiceDiscovery.UnitTestInjection(sd);

            return sd;
        }

        [Test]
        public void GetHypervisorNames_DefaultState_ReturnsListOfAvailableHypervisors()
        {
            var serviceDiscovery = CreateFakeServiceDiscovery();
            var hypervisor1 = A.Fake<IHypervisorInfo>();
            var hypervisor2 = A.Fake<IHypervisorInfo>();
            var sut = DefaultHypervisorFactory();
            A.CallTo(() => serviceDiscovery.ResolveAll<IHypervisorInfo>()).Returns(new[] {hypervisor1, hypervisor2});
            A.CallTo(() => hypervisor1.Name).Returns("MyHypervisor1");
            A.CallTo(() => hypervisor2.Name).Returns("MyHypervisor2");

            var results = sut.GetHypervisorNames().ToList();

            Assert.Contains("MyHypervisor1", results);
            Assert.Contains("MyHypervisor2", results);
        }

        [Test]
        public void CreateHypervisorConnectionInfo_ValidHypervisorPassedIn_ObjectReturned()
        {
            var srvdisc = CreateFakeServiceDiscovery();
            var hypervisor = A.Fake<IHypervisorInfo>();
            var connectioninfo = A.Fake<IHypervisorConnectionInfo>();
            var sut = DefaultHypervisorFactory();
            A.CallTo(() => srvdisc.ResolveAll<IHypervisorInfo>()).Returns(new[] {hypervisor});
            A.CallTo(() => hypervisor.Name).Returns("MyHypervisor");
            A.CallTo(() => hypervisor.CreateConnectionInfo()).Returns(connectioninfo);

            var result = sut.CreateHypervisorConnectionInfo("MyHypervisor");

            Assert.That(result == connectioninfo);
        }

        [Test]
        public void CreateHypervisorConnectionInfo_InvalidHypervisorNamePassedIn_WillThrow()
        {
            var srvdisc = CreateFakeServiceDiscovery();
            var sut = DefaultHypervisorFactory();
            A.CallTo(() => srvdisc.ResolveAll<IHypervisorInfo>()).Returns(new IHypervisorInfo[] {});

            Assert.Throws<UnknownHypervisorException>(() =>  sut.CreateHypervisorConnectionInfo("NonexistingHypervisor"));
        }

        [Test]
        public void CreateHypervisor_ValidHypervisorNamePassedIn_HypervisorObjectReturned()
        {
            var srvdisc = CreateFakeServiceDiscovery();
            var hypervisorinfo = A.Fake<IHypervisorInfo>();
            var hypervisor = A.Fake<IHypervisor>();
            var connectioninfo = A.Fake<IHypervisorConnectionInfo>();
            var sut = DefaultHypervisorFactory();
            A.CallTo(() => hypervisorinfo.Name).Returns("MyHypervisor");
            A.CallTo(() => srvdisc.ResolveAll<IHypervisorInfo>()).Returns(new[] {hypervisorinfo});
            A.CallTo(() => hypervisorinfo.CreateHypervisor(A<IHypervisorConnectionInfo>.Ignored)).Returns(hypervisor);

            var result = sut.CreateHypervisor("MyHypervisor", connectioninfo);

            Assert.That(result == hypervisor);
        }

        [Test]
        public void CreateHypervisor_NonExistingHypervisorNameIsPassedIn_WillThrow()
        {
            CreateFakeServiceDiscovery();
            var connectioninfo = A.Fake<IHypervisorConnectionInfo>();
            var sut = DefaultHypervisorFactory();

            Assert.Throws<UnknownHypervisorException>(() => sut.CreateHypervisor("NonExistingHypervisor", connectioninfo));
        }
    }
}
