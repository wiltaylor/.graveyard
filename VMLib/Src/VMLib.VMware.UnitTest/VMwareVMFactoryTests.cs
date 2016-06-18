using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using SystemWrapper.IO;
using FakeItEasy;
using NUnit.Framework;
using VMLib.Exceptions;
using VMLib.IOC;
using VMLib.UnitTest;
using VMLib.VMware.VIXItems;

namespace VMLib.VMware.UnitTest
{
    [TestFixture]
    public class VMwareVMFactoryTests
    {

        public IVMFactory DefaultVMwareVMFactory(IFileWrap file = null, IServiceDiscovery srvDiscovery = null, IVix vix = null)
        {
            if (file == null)
                file = A.Fake<IFileWrap>();

            if (srvDiscovery == null)
                srvDiscovery = FakeServiceDiscovery.ReturnTestableInstance();

            if (vix == null)
                vix = A.Fake<IVix>();

            var sut = new VMwareVMFactory(file, vix);

            A.CallTo(() => file.Exists("c:\\goodfilepath.vmx")).Returns(true);

            return sut;
        }

        public IVMCreationInfo DefaultCreationInfo()
        {
            var fake = A.Fake<IVMCreationInfo>();
            fake.Name = "FakeVM";
            fake.CPU = 1;
            fake.Cores = 1;
            fake.Memory = 1024;
            fake.Path = "c:\\goodfilepath.vmx";
            return fake;
        }

        [Test]
        public void Create_CallingCreateVMWithNull_WillThrow()
        {
            var sut = DefaultVMwareVMFactory();

            Assert.Throws<ArgumentNullException>(() => sut.Create(null));
        }

        [Test]
        public void Create_CallingWithNullPath_WillThrow()
        {
            var sut = DefaultVMwareVMFactory();
            var info = DefaultCreationInfo();
            info.Path = null;

            Assert.Throws<InvalidVMCreationInfoException>(() => sut.Create(info));
        }

        [TestCase(0)]
        [TestCase(-1)]
        public void Create_CallingWithCPULessThan1_WillThrow(int qty)
        {
            var sut = DefaultVMwareVMFactory();
            var info = DefaultCreationInfo();
            info.CPU = qty;

            Assert.Throws<InvalidVMCreationInfoException>(() => sut.Create(info));
        }

        [TestCase(0)]
        [TestCase(-1)]
        public void Create_CallingWithCoresLessThan1_WillThrow(int qty)
        {
            var sut = DefaultVMwareVMFactory();
            var info = DefaultCreationInfo();
            info.Cores = qty;

            Assert.Throws<InvalidVMCreationInfoException>(() => sut.Create(info));
        }

        [TestCase(0)]
        [TestCase(-1)]
        public void Create_CallingWithMemoryLessThan1_WillThrow(int qty)
        {
            var sut = DefaultVMwareVMFactory();
            var info = DefaultCreationInfo();
            info.Memory = qty;

            Assert.Throws<InvalidVMCreationInfoException>(() => sut.Create(info));
        }

        [Test]
        public void Create_CallingWithNullName_WillThrow()
        {
            var sut = DefaultVMwareVMFactory();
            var info = DefaultCreationInfo();
            info.Name = null;

            Assert.Throws<InvalidVMCreationInfoException>(() => sut.Create(info));
        }

        [TestCase(".encoding", "windows-1252")]
        [TestCase("config.version", "8")]
        [TestCase("virtualHW.version", "12")]
        public void Create_WillWriteRequiredProperties_WillGenerateVMX(string setting, string value)
        {
            var srv = FakeServiceDiscovery.ReturnTestableInstance();
            var sut = DefaultVMwareVMFactory(srvDiscovery: srv);
            var info = DefaultCreationInfo();
            var vmxhelper = A.Fake<IVMXHelper>();
            A.CallTo(() => srv.Resolve<IVMXHelper>("VMwareWorkstation")).Returns(vmxhelper);

            sut.Create(info);

            A.CallTo(() => vmxhelper.WriteVMX(setting, value)).MustHaveHappened();
        }

        [TestCase(1, 1)]
        [TestCase(1, 2)]
        [TestCase(2, 2)]
        [TestCase(5, 2)]
        public void Create_CallingWithCPUAndCore_WillGenerateVMX(int cpu, int core)
        {
            var srv = FakeServiceDiscovery.ReturnTestableInstance();
            var sut = DefaultVMwareVMFactory(srvDiscovery: srv);
            var info = DefaultCreationInfo();
            var vmxhelper = A.Fake<IVMXHelper>();
            A.CallTo(() => srv.Resolve<IVMXHelper>("VMwareWorkstation")).Returns(vmxhelper);
            info.CPU = cpu;
            info.Cores = core;
            var vmxcpu = cpu*core; //VMware VMX files have a maximum number of vcpus and then you specify how many cores per cpu.
            var vmxcore = core;
            
            sut.Create(info);

            A.CallTo(() => vmxhelper.WriteVMX("numvcpus", vmxcpu.ToString())).MustHaveHappened();
            A.CallTo(() => vmxhelper.WriteVMX("cpuid.coresPerSocket", vmxcore.ToString())).MustHaveHappened();
        }

        [TestCase(512)]
        [TestCase(1024)]
        [TestCase(2048)]
        public void Create_CallingWithMemory_WillGenerateVMX(int qty)
        {
            var srv = FakeServiceDiscovery.ReturnTestableInstance();
            var sut = DefaultVMwareVMFactory(srvDiscovery: srv);
            var info = DefaultCreationInfo();
            var vmxhelper = A.Fake<IVMXHelper>();
            A.CallTo(() => srv.Resolve<IVMXHelper>("VMwareWorkstation")).Returns(vmxhelper);
            info.Memory = qty;

            sut.Create(info);

            A.CallTo(() => vmxhelper.WriteVMX("memsize", qty.ToString())).MustHaveHappened();
        }

        [Test]
        public void Create_CallingWithName_WillGenerateVMX()
        {
            var srv = FakeServiceDiscovery.ReturnTestableInstance();
            var sut = DefaultVMwareVMFactory(srvDiscovery: srv);
            var info = DefaultCreationInfo();
            var vmxhelper = A.Fake<IVMXHelper>();
            A.CallTo(() => srv.Resolve<IVMXHelper>("VMwareWorkstation")).Returns(vmxhelper);
            info.Name = "MyVM";

            sut.Create(info);

            A.CallTo(() => vmxhelper.WriteVMX("displayName", "MyVM")).MustHaveHappened();
        }

        [TestCase(GuestOS.WindowsXPx86, "winxppro")]
        [TestCase(GuestOS.WindowsXPx64, "winxppro-64")]
        [TestCase(GuestOS.Windows2003x86, "winnetenterprise")]
        [TestCase(GuestOS.Windows2003x64, "winnetenterprise-64")]
        [TestCase(GuestOS.WindowsVistax86, "winvista")]
        [TestCase(GuestOS.WindowsVistax64, "winvista-64")]
        [TestCase(GuestOS.Windows2008x86, "longhorn")]
        [TestCase(GuestOS.Windows2008x64, "longhorn-64")]
        [TestCase(GuestOS.Windows7x86, "windows7")]
        [TestCase(GuestOS.Windows7x64, "windows7-64")]
        [TestCase(GuestOS.Windows2008R2x64, "windows7srv-64")]
        [TestCase(GuestOS.Windows8x64, "windows8")]
        [TestCase(GuestOS.Windows8x86, "windows8-64")]
        [TestCase(GuestOS.Windows2012x64, "windows8srv-64")]
        [TestCase(GuestOS.Windows81x86, "windows8")]
        [TestCase(GuestOS.Windows81x64, "windows8-64")]
        [TestCase(GuestOS.Windows2012R2x64, "windows8srv-64")]
        [TestCase(GuestOS.Windows10x86, "windows9")]
        [TestCase(GuestOS.Windows10x64, "windows9-64")]
        [TestCase(GuestOS.Windows2016x64, "windows9-64")]
        public void Create_CallingWithGuestOS_WillSetExpectedOS(GuestOS os, string vmwareguestosstring)
        {
            var srv = FakeServiceDiscovery.ReturnTestableInstance();
            var sut = DefaultVMwareVMFactory(srvDiscovery: srv);
            var info = DefaultCreationInfo();
            var vmxhelper = A.Fake<IVMXHelper>();
            A.CallTo(() => srv.Resolve<IVMXHelper>("VMwareWorkstation")).Returns(vmxhelper);
            info.GuestOS = os;

            sut.Create(info);

            A.CallTo(() => vmxhelper.WriteVMX("guestOS", vmwareguestosstring)).MustHaveHappened();
        }

        [Test]
        public void Create_CallingWithNetworkCards_WillCallIVMXHelperToWriteVMX()
        {
            var srv = FakeServiceDiscovery.ReturnTestableInstance();
            var sut = DefaultVMwareVMFactory(srvDiscovery: srv);
            var info = DefaultCreationInfo();
            var vmxhelper = A.Fake<IVMXHelper>();
            var network = A.Fake<IVMNetwork>();
            A.CallTo(() => srv.Resolve<IVMXHelper>("VMwareWorkstation")).Returns(vmxhelper);
            A.CallTo(() => info.Networks).Returns(new[] {network});

            sut.Create(info);

            A.CallTo(() => vmxhelper.WriteNetwork(network)).MustHaveHappened();
        }

        [Test]
        public void Create_CallingWithDisks_WillCallIVMXHelperToWriteVMX()
        {
            var srv = FakeServiceDiscovery.ReturnTestableInstance();
            var sut = DefaultVMwareVMFactory(srvDiscovery: srv);
            var info = DefaultCreationInfo();
            var vmxhelper = A.Fake<IVMXHelper>();
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => srv.Resolve<IVMXHelper>("VMwareWorkstation")).Returns(vmxhelper);
            A.CallTo(() => info.Disks).Returns(new[] { disk });

            sut.Create(info);

            A.CallTo(() => vmxhelper.WriteDisks(disk)).MustHaveHappened();
        }

        [Test]
        public void Create_CallingWithCustomSettings_WillSetThem()
        {
            var srv = FakeServiceDiscovery.ReturnTestableInstance();
            var sut = DefaultVMwareVMFactory(srvDiscovery: srv);
            var info = DefaultCreationInfo();
            var vmxhelper = A.Fake<IVMXHelper>();
            A.CallTo(() => srv.Resolve<IVMXHelper>("VMwareWorkstation")).Returns(vmxhelper);
            info.CustomSettings = new Dictionary<string, string>
            {
                { "MySetting", "MyValue"}
            };

            sut.Create(info);

            A.CallTo(() => vmxhelper.WriteVMX("MySetting", "MyValue")).MustHaveHappened();
        }

        [Test]
        public void Create_AfterAllVMXSettingsCreated_StringArrayReturned()
        {
            var srv = FakeServiceDiscovery.ReturnTestableInstance();
            var sut = DefaultVMwareVMFactory(srvDiscovery: srv);
            var info = DefaultCreationInfo();
            var vmxhelper = A.Fake<IVMXHelper>();
            
            A.CallTo(() => srv.Resolve<IVMXHelper>("VMwareWorkstation")).Returns(vmxhelper);
            
            sut.Create(info);

            A.CallTo(() => vmxhelper.ToArray()).MustHaveHappened();
        }

        [Test]
        public void Create_WriteVMXDataToFile_FileObjectCalled()
        {
            var srv = FakeServiceDiscovery.ReturnTestableInstance();
            var file = A.Fake<IFileWrap>();
            var sut = DefaultVMwareVMFactory(file: file);
            var info = DefaultCreationInfo();
            var vmxhelper = A.Fake<IVMXHelper>();
            A.CallTo(() => srv.Resolve<IVMXHelper>("VMwareWorkstation")).Returns(vmxhelper);
            A.CallTo(() => vmxhelper.ToArray()).Returns(new[] {"line1", "line2"});
            A.CallTo(() => file.Exists("c:\\vmfolder\\vm.vmx")).Returns(true);
            info.Path = "c:\\vmfolder\\vm.vmx";


            sut.Create(info);

            A.CallTo(() => file.WriteAllLines("c:\\vmfolder\\vm.vmx", A<string[]>.Ignored)).MustHaveHappened();
        }

        [Test]
        public void Create_CreatesInstanceOfVMwareVirtualMachine_IsNotNull()
        {
            var sut = DefaultVMwareVMFactory();
            var info = DefaultCreationInfo();

            var result = sut.Create(info);

            Assert.IsNotNull(result);
        }

        [Test]
        public void Open_PassedNullPath_WillThrow()
        {
            var sut = DefaultVMwareVMFactory();

            Assert.Throws<ArgumentNullException>(() => sut.Open(null));
        }

        [Test]
        public void Open_NonExistingPath_WillThrow()
        {
            var file = A.Fake<IFileWrap>();
            var sut = DefaultVMwareVMFactory(file: file);
            A.CallTo(() => file.Exists("c:\\badfilepath.vmx")).Returns(false);

            Assert.Throws<FileNotFoundException>(() => sut.Open("c:\\badfilepath.vmx"));
        }

        [Test]
        public void GetAllRunning_WillCallVix_ReturnsVMObjectsForAllOpen()
        {
            var file = A.Fake<IFileWrap>();
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVMFactory(file: file, vix: vix);
            A.CallTo(() => vix.GetAllRunning()).Returns(new[] {"c:\\vm1.vmx", "c:\\vm2.vmx"});
            A.CallTo(() => file.Exists("c:\\vm1.vmx")).Returns(true);
            A.CallTo(() => file.Exists("c:\\vm2.vmx")).Returns(true);

            var resault = sut.GetAllRunning();

            Assert.That(resault.Count() == 2);
        }

        [Test]
        public void SetConnectionInfo_WillStoreSettings_WillNotThrow()
        {
            var sut = DefaultVMwareVMFactory();
            var info = A.Fake<IHypervisorConnectionInfo>();

            Assert.DoesNotThrow(() => sut.SetConnectionInfo(info));
        }
    }
}

