using SystemWrapper.IO;
using FakeItEasy;
using NUnit.Framework;
using VMLib.Exceptions;
using VMLib.UnitTest;

namespace VMLib.VMware.UnitTest
{
    [TestFixture]
    public class VMwareHypervisorTests
    {

        public IHypervisor DefaultVMwareHypervisorFactory(IHypervisorConnectionInfo connectionInfo = null)
        {
            if (connectionInfo == null)
                connectionInfo = A.Fake<IHypervisorConnectionInfo>();

            var sut = new VMwareHypervisor(A.Fake<IDirectoryWrap>());
            sut.SetConnectionSettings(connectionInfo);

            return sut;
        }

        public IVMCreationInfo DefaultVMwareCreationInfo()
        {
            var fake = A.Fake<IVMCreationInfo>();

            return fake;
        }

        [Test]
        public void Name_CheckNameProperty_IsVMwareWorkstation()
        {
            var sut = DefaultVMwareHypervisorFactory();

            Assert.That(sut.Name == "VMwareWorkstation");
        }
    }
}
