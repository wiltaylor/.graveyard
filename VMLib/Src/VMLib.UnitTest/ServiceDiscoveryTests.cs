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

        [Test]
        public void AddType_AddingType_CanRecoverTypeByCallingResolve()
        {
            var sut = DefaultServiceDiscoveryFactory();
            
            sut.AddType<ITest,Test1>("MyName");
            var result = sut.Resolve<ITest>("MyName");


            Assert.IsInstanceOf<Test1>(result);
        }

        [Test]
        public void AddType_AddingMultipleTypes_CanRecoverByCallingResolveAll()
        {
            var sut = DefaultServiceDiscoveryFactory();

            sut.AddType<ITest, Test1>();
            sut.AddType<ITest, Test2>();
            var result = sut.ResolveAll<ITest>();
            
            Assert.That(result.Count() == 2);
        }

        [Test]
        public void AddType_WithoutName_WillReturnObjectWithoutName()
        {
            var sut = DefaultServiceDiscoveryFactory();

            sut.AddType<ITest, Test1>();
            var result = sut.Resolve<ITest>();

            Assert.IsInstanceOf<Test1>(result);
        }
    }
}
