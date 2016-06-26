using System;
using FakeItEasy;
using NUnit.Framework;
using VMLib.Exceptions;

namespace VMLib.UnitTest
{
    [TestFixture]
    public class HypervisorBaseTests
    {
        public IHypervisor DefaultTestableHypervisorFactory()
        {
            var sut = new TestableHypervisorBase();

            return sut;
        }

        [Test]
        public void SetConnectionSettings_CallingOnce_WillNotThrow()
        {
            var sut = DefaultTestableHypervisorFactory();

            Assert.DoesNotThrow(() => sut.SetConnectionSettings(A.Fake<IHypervisorConnectionInfo>()));
        }

        [Test]
        public void SetConnectionSettings_CallingMultipleTimes_WillThrow()
        {
            var sut = DefaultTestableHypervisorFactory();

            sut.SetConnectionSettings(A.Fake<IHypervisorConnectionInfo>());
            Assert.Throws<HypervisorAlreadySetupException>(() => sut.SetConnectionSettings(A.Fake<IHypervisorConnectionInfo>()));

        }

        [Test]
        public void CreateNewVM_CallingMethod_MakesCallToFactory()
        {
            var srv = FakeServiceDiscovery.ReturnTestableInstance();
            var sut = DefaultTestableHypervisorFactory();
            var factory = A.Fake<IVMFactory>();
            A.CallTo(() => srv.Resolve<IVMFactory>(sut.Name)).Returns(factory);

            sut.CreateNewVM(A.Fake<IVMCreationInfo>());

            A.CallTo(() => factory.Create(A<IVMCreationInfo>.Ignored)).MustHaveHappened();
        }

        [Test]
        public void OpenVM_CallingMethod_MakesCallToFactory()
        {
            var srv = FakeServiceDiscovery.ReturnTestableInstance();
            var sut = DefaultTestableHypervisorFactory();
            var factory = A.Fake<IVMFactory>();
            A.CallTo(() => srv.Resolve<IVMFactory>(sut.Name)).Returns(factory);

            sut.OpenVM("c:\\Fake\\Path\\ToVM.Whateverextension");

            A.CallTo(() => factory.Open("c:\\Fake\\Path\\ToVM.Whateverextension")).MustHaveHappened();
        }

        [Test]
        public void GetAllRunningVM_CallingMethod_MakesCallToFactory()
        {
            var srv = FakeServiceDiscovery.ReturnTestableInstance();
            var sut = DefaultTestableHypervisorFactory();
            var factory = A.Fake<IVMFactory>();
            A.CallTo(() => srv.Resolve<IVMFactory>(sut.Name)).Returns(factory);

            sut.GetAllRunningVM();

            A.CallTo(() => factory.GetAllRunning()).MustHaveHappened();
        }

        [Test]
        public void AnyMethodWithFactory_PassesConnectionInfoToFactory_FactorySetConnectionInfoCalled()
        {
            var srv = FakeServiceDiscovery.ReturnTestableInstance();
            var sut = DefaultTestableHypervisorFactory();
            var factory = A.Fake<IVMFactory>();
            A.CallTo(() => srv.Resolve<IVMFactory>(sut.Name)).Returns(factory);

            sut.CreateNewVM(A.Fake<IVMCreationInfo>());

            A.CallTo(() => factory.SetConnectionInfo(A<IHypervisorConnectionInfo>.Ignored)).MustHaveHappened();
        }

    }

    public class TestableHypervisorBase : HypervisorBase
    {
        public override string Name => "Testable";
    }
}
