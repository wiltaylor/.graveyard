using System;
using FakeItEasy;
using NUnit.Framework;
using VMLib.IOC;

namespace VMLib.UnitTest
{


    [TestFixture]
    public class HypervisorInfoBaseTests 
    {
        public IServiceDiscovery DefaultServiceDiscovery()
        {
            var srv = A.Fake<IServiceDiscovery>();
            ServiceDiscovery.UnitTestInjection(srv);
            return srv;
        }

        public TesableHyperVisorBase DefaultTestableHypervisorInfoBase()
        {
            var sut = new TesableHyperVisorBase();

            return sut;

        }

        [Test]
        public void AddTypeToIOC_AddingATypeToIOC_CallsServiceDiscovery()
        {
            var srv = DefaultServiceDiscovery();
            var sut = DefaultTestableHypervisorInfoBase();
            
            sut.TestInternal_AddTypeToIOC<IHypervisor, FakeHypervisor>();

            A.CallTo(() => srv.AddType<IHypervisor, FakeHypervisor>("Testable")).MustHaveHappened();
        }

    }

    public class FakeHypervisor : IHypervisor
    {
        
    }

    public class TesableHyperVisorBase : HypervisorInfoBase
    {
        public override string Name => "Testable";

        public void TestInternal_AddTypeToIOC<F, T>()
        {
            AddTypeToIOC<F,T>();
        }

        public override IHypervisorConnectionInfo CreateConnectionInfo()
        {
            throw new NotImplementedException();
        }

        public override IHypervisor CreateHypervisor(IHypervisorConnectionInfo info)
        {
            throw new NotImplementedException();
        }
    }
}
