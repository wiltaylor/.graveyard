using FakeItEasy;
using NUnit.Framework;

namespace VMLib.VirtualBox.UnitTest
{
    [TestFixture]
    public class VirtualBoxHypervisorTests
    {

        public IHypervisor DefaultVirtualBoxHypervisorFactory(IHypervisorConnectionInfo connectionInfo = null)
        {
            if (connectionInfo == null)
                connectionInfo = A.Fake<IHypervisorConnectionInfo>();

            var sut = new VirtualBoxHypervisor();
            sut.SetConnectionSettings(connectionInfo);
            return sut;
        }

        [Test]
        public void Name_CheckNameProperty_IsVMwareWorkstation()
        {
            var sut = DefaultVirtualBoxHypervisorFactory();

            Assert.That(sut.Name == "VirtualBox");
        }
    }
}
