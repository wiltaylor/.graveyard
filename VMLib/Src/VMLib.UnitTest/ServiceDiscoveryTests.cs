using System.Linq;
using NUnit.Framework;
using VMLib.IOC;

namespace VMLib.UnitTest
{

    public interface ITest
    {
        
    }

    public class Test1 : ITest
    {
        
    }

    public class Test2 : ITest
    {
        
    }

    [TestFixture]
    public class ServiceDiscoveryTests
    {

        public IServiceDiscovery DefaultServiceDiscoveryFactory()
        {
            var sut = new ServiceDiscovery();
            ServiceDiscovery.UnitTestInjection(sut);
            return sut;
        }

    }
}
