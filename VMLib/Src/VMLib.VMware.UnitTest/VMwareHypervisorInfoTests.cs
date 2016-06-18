using System.Linq;
using SystemWrapper.Microsoft.Win32;
using FakeItEasy;
using NUnit.Framework;

namespace VMLib.VMware.UnitTest
{
    [TestFixture]
    public class VMwareHypervisorInfoTests
    {
        public IHypervisorInfo DefaultVMwareHypervisorInfoFactory(IRegistryWrap reg = null)
        {
            if (reg == null)
                reg = A.Fake<IRegistryWrap>();

            var sut = new VMwareHypervisorInfo(reg);
            
            return sut;
        }

        [Test]
        public void Name_CheckNameProperty_IsVMwareWorkstation()
        {
            var sut = DefaultVMwareHypervisorInfoFactory();

            Assert.That(sut.Name == "VMwareWorkstation");
        }

        [TestCase("VMwareWorkstationPath")]
        public void CreateConnectionInfo_CheckForExpectedProperties_ContainsExpectedProperties(string property)
        {
            var sut = DefaultVMwareHypervisorInfoFactory();

            var result = sut.CreateConnectionInfo();

            Assert.That(result.Properties.Keys.Contains(property));
        }

        [TestCase("SOFTWARE\\WOW6432Node\\VMware, Inc.\\VMware Workstation")]
        [TestCase("SOFTWARE\\VMware, Inc.\\VMware Workstation")]
        public void CreateConnectionInfo_ReadsRegistryKeyToFindVMwareWorkstaionPath_ReturnsPathFromRegistry(string keypath)
        {
            var reg = A.Fake<IRegistryWrap>();
            var hklm = A.Fake<IRegistryKeyWrap>();
            var vmwarekey = A.Fake<IRegistryKeyWrap>();
            var sut = DefaultVMwareHypervisorInfoFactory(reg: reg);
            A.CallTo(() => reg.LocalMachine).Returns(hklm);
            A.CallTo(() => hklm.OpenSubKey(keypath)).Returns(vmwarekey);
            A.CallTo(() => vmwarekey.GetValue("InstallPath")).Returns("C:\\Program Files (x86)\\VMware\\VMware Workstation\\");

            var result = sut.CreateConnectionInfo();

            Assert.That(result.Properties["VMwareWorkstationPath"] == "C:\\Program Files (x86)\\VMware\\VMware Workstation\\");
        }
    }
}
