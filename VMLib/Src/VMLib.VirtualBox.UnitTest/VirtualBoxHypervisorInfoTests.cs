using System.Linq;
using SystemWrapper.Microsoft.Win32;
using FakeItEasy;
using NUnit.Framework;
using VMLib.IOC;
using VMLib.UnitTest;

namespace VMLib.VirtualBox.UnitTest
{
    [TestFixture]
    public class VirtualBoxHypervisorInfoTests
    {
        public IHypervisorInfo DefaultVirtualBoxHypervisorInfo(IRegistryWrap reg = null)
        {
            if (reg == null)
                reg = A.Fake<IRegistryWrap>();

            var sut = new VirtualBoxHypervisorInfo(reg);

            return sut;
        }
        [Test]
        public void Name_CheckNameProperty_IsVirtualBox()
        {
            var sut = DefaultVirtualBoxHypervisorInfo();

            Assert.That(sut.Name == "VirtualBox");
        }

        [Test]
        public void Constructor_RegistersVirtualBoxHypervisorDuringConstruction()
        {
            var srv = FakeServiceDiscovery.ReturnTestableInstance();

            var sut = DefaultVirtualBoxHypervisorInfo();

            A.CallTo(() => srv.AddType<IHypervisor, VirtualBoxHypervisor>(sut.Name)).MustHaveHappened();
        }

        [TestCase("VirtualBoxPath")]
        public void CreateConnectionInfo_CheckForExpectedProperties_ContainsExpectedPRoperties(string property)
        {
            var sut = DefaultVirtualBoxHypervisorInfo();

            var result = sut.CreateConnectionInfo();

            Assert.That(result.Properties.Keys.Contains(property));
        }

        [TestCase("SOFTWARE\\Oracle\\VirtualBox")]
        public void CreateConnectionInfo_ReadsRegistryKeyToFindVMwareWorkstationPath_ReturnsPathFromREgistry(
            string keypath)
        {
            var reg = A.Fake<IRegistryWrap>();
            var hklm = A.Fake<IRegistryKeyWrap>();
            var vbkey = A.Fake<IRegistryKeyWrap>();
            var sut = DefaultVirtualBoxHypervisorInfo(reg: reg);
            A.CallTo(() => reg.LocalMachine).Returns(hklm);
            A.CallTo(() => hklm.OpenSubKey(keypath)).Returns(vbkey);
            A.CallTo(() => vbkey.GetValue("InstallDir")).Returns("C:\\Program Files\\Oracle\\VirtualBox\\");

            var result = sut.CreateConnectionInfo();

            Assert.That(result.Properties["VirtualBoxPath"] == "C:\\Program Files\\Oracle\\VirtualBox\\");
        }
    }
}
