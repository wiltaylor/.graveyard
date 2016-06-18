using FakeItEasy;
using VMLib.IOC;

namespace VMLib.UnitTest
{
    public static class FakeServiceDiscovery
    {
        public static IServiceDiscovery ReturnTestableInstance()
        {
            var fake = A.Fake<IServiceDiscovery>();
            ServiceDiscovery.UnitTestInjection(fake);

            return fake;
        }
    }
}
