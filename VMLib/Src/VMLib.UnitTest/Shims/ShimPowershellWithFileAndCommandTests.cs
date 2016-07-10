using System;
using System.Collections;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using SystemWrapper.IO;
using FakeItEasy;
using NUnit.Framework;
using VMLib.IOC;
using VMLib.Shims;
using VMLib.Utility;

namespace VMLib.UnitTest.Shims
{
    [TestFixture]
    public class ShimPowershellWithFileAndCommandTests
    {
        public IVirtualMachine DefaultShimFactory(IVirtualMachine subvm = null, IServiceDiscovery svc = null)
        {
            if (subvm == null)
                subvm = A.Fake<IVirtualMachine>();

            if (svc == null)
                svc = DefaultServiceDiscoveryFactory();

            return new ShimPowershellWithFileAndCommand(subvm);
        }

        public IServiceDiscovery DefaultServiceDiscoveryFactory(IFileWrap file = null, IEnvironmentHelper env = null)
        {
            var fake = A.Fake<IServiceDiscovery>();
            ServiceDiscovery.UnitTestInjection(fake);

            if (file != null)
                A.CallTo(() => fake.Resolve<IFileWrap>()).Returns(file);
            if (env == null)
                env = DefaultEnvironmentFactory();

            A.CallTo(() => fake.Resolve<IEnvironmentHelper>()).Returns(env);

            return fake;
        }

        public IEnvironmentHelper DefaultEnvironmentFactory(string getguid = "{RANDOMGUID}", string tempdir = "c:\\temp")
        {
            var fake = A.Fake<IEnvironmentHelper>();

            A.CallTo(() => fake.GetGUID()).Returns(getguid);
            A.CallTo(() => fake.TempDir).Returns(tempdir);

            return fake;
        }

        [Test]
        public void ExecutePowershell_TemporyScriptFile_CreatesFileInTemp()
        {
            var file = A.Fake<IFileWrap>();
            var env = DefaultEnvironmentFactory(getguid: "{RANDOMGUID}", tempdir: "c:\\temp");
            var svc = DefaultServiceDiscoveryFactory(file: file, env: env);
            var sut = DefaultShimFactory(svc: svc);

            sut.ExecutePowershell("#MyScript", null);

            A.CallTo(() => file.WriteAllText("c:\\temp\\{RANDOMGUID}.ps1", "#MyScript")).MustHaveHappened();
        }

        [Test]
        public void ExecutePowershell_SerialiseHashTable_CreateSerialisedFile()
        {
            var file = A.Fake<IFileWrap>();
            var env = DefaultEnvironmentFactory(getguid: "{RANDOMGUID}", tempdir: "c:\\temp");
            var svc = DefaultServiceDiscoveryFactory(file: file, env: env);
            var sut = DefaultShimFactory(svc: svc);

            sut.ExecutePowershell("#MyScript", new Hashtable
            {
                {"MyValueName","MyValue" }
            });

            A.CallTo(() => file.WriteAllText("c:\\temp\\{RANDOMGUID}.data.xml", A<string>.Ignored)).MustHaveHappened();
        }

        [Test]
        public void ExecutePowershell_WriteBatchFile_WillWriteToBatchFile()
        {
            var file = A.Fake<IFileWrap>();
            var env = DefaultEnvironmentFactory(getguid: "{RANDOMGUID}", tempdir: "c:\\temp");
            var svc = DefaultServiceDiscoveryFactory(file: file, env: env);
            var subvm = A.Fake<IVirtualMachine>();
            var sut = DefaultShimFactory(svc: svc, subvm: subvm);

            sut.ExecutePowershell("#MyScript", null);

            var script = "@\"C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe\" -executionpolicy bypass -noprofile -noninteractive -file \"c:\\windows\\temp\\{RANDOMGUID}.ps1\" > \"c:\\windows\\temp\\{RANDOMGUID}.stdout\" 2> \"c:\\windows\\temp\\{RANDOMGUID}.stderr\"";
            A.CallTo(() => file.WriteAllText("c:\\temp\\{RANDOMGUID}.cmd", script)).MustHaveHappened();
        }


        [Test]
        public void ExecutePowershell_VMLibArgsPresent_WillPatchScriptToRetriveArgs()
        {
            var file = A.Fake<IFileWrap>();
            var env = DefaultEnvironmentFactory(getguid: "{RANDOMGUID}", tempdir: "c:\\temp");
            var svc = DefaultServiceDiscoveryFactory(file: file, env: env);
            var sut = DefaultShimFactory(svc: svc);

            sut.ExecutePowershell("#MyScript\r\n$args = get-vmlibargs", new Hashtable
            {
                {"MyValueName","MyValue" }
            });

            A.CallTo(() => file.WriteAllText("c:\\temp\\{RANDOMGUID}.ps1", "#MyScript\r\n$args = Import-CliXml \"c:\\windows\\temp\\{RANDOMGUID}.data.xml\"")).MustHaveHappened();
        }

        [TestCase("c:\\temp\\{RANDOMGUID}.ps1", "c:\\windows\\temp\\{RANDOMGUID}.ps1")]
        [TestCase("c:\\temp\\{RANDOMGUID}.cmd", "c:\\windows\\temp\\{RANDOMGUID}.cmd")]
        [TestCase("c:\\temp\\{RANDOMGUID}.data.xml", "c:\\windows\\temp\\{RANDOMGUID}.data.xml")]
        public void ExecutePowershell_CopyScriptToVM_SubVMCalledToCopyFilesToVM(string local, string guest)
        {
            var env = DefaultEnvironmentFactory(getguid: "{RANDOMGUID}", tempdir: "c:\\temp");
            var svc = DefaultServiceDiscoveryFactory(env: env);
            var subvm = A.Fake<IVirtualMachine>();
            var sut = DefaultShimFactory(svc: svc, subvm: subvm);

            sut.ExecutePowershell("#MyScript", new Hashtable
            {
                {"MyValueName","MyValue" }
            });

            A.CallTo(() => subvm.CopyToVM(local, guest))
                .MustHaveHappened();
        }

        [Test]
        public void ExecutePowershell_ExecuteScriptOnGuest_CallBaseVMToExecute()
        {
            var env = DefaultEnvironmentFactory(getguid: "{RANDOMGUID}", tempdir: "c:\\temp");
            var svc = DefaultServiceDiscoveryFactory(env: env);
            var subvm = A.Fake<IVirtualMachine>();
            var sut = DefaultShimFactory(svc: svc, subvm: subvm);

            sut.ExecutePowershell("#MyScript", null);

            A.CallTo(() =>
                subvm.ExecuteCommand("c:\\windows\\system32\\cmd.exe", "/c \"c:\\windows\\temp\\{RANDOMGUID}.cmd\"",
                    true, false)).MustHaveHappened();
        }

        [TestCase("c:\\windows\\temp\\{RANDOMGUID}.stdout", "c:\\temp\\{RANDOMGUID}.stdout")]
        [TestCase("c:\\windows\\temp\\{RANDOMGUID}.stderr", "c:\\temp\\{RANDOMGUID}.stderr")]
        public void ExecutePowershell_CopyFiles_FromGuest(string guestpath, string hostpath)
        {
            var env = DefaultEnvironmentFactory(getguid: "{RANDOMGUID}", tempdir: "c:\\temp");
            var svc = DefaultServiceDiscoveryFactory(env: env);
            var subvm = A.Fake<IVirtualMachine>();
            var sut = DefaultShimFactory(svc: svc, subvm: subvm);
            A.CallTo(() => subvm.FileExists(guestpath)).Returns(true);

            sut.ExecutePowershell("#MyScript", null);

            A.CallTo(() => subvm.CopyFromVM(guestpath, hostpath))
                .MustHaveHappened();
        }

        [TestCase("c:\\windows\\temp\\{RANDOMGUID}.stdout", "c:\\temp\\{RANDOMGUID}.stdout")]
        [TestCase("c:\\windows\\temp\\{RANDOMGUID}.stderr", "c:\\temp\\{RANDOMGUID}.stderr")]
        public void ExecutePowershell_GuestFilesDontExist_FilesNotCopied(string guestpath, string hostpath)
        {
            var env = DefaultEnvironmentFactory(getguid: "{RANDOMGUID}", tempdir: "c:\\temp");
            var svc = DefaultServiceDiscoveryFactory(env: env);
            var subvm = A.Fake<IVirtualMachine>();
            var sut = DefaultShimFactory(svc: svc, subvm: subvm);
            A.CallTo(() => subvm.FileExists(guestpath)).Returns(false);

            sut.ExecutePowershell("#MyScript", null);

            A.CallTo(() => subvm.CopyFromVM(guestpath, hostpath)).MustNotHaveHappened();
        }

        [Test]
        public void ExceutePowershell_STDOutReturned_STDInResult()
        {
            var file = A.Fake<IFileWrap>();
            var env = DefaultEnvironmentFactory(getguid: "{RANDOMGUID}", tempdir: "c:\\temp");
            var svc = DefaultServiceDiscoveryFactory(file: file, env: env);
            var sut = DefaultShimFactory(svc: svc);
            const string xmldata = @"
                <Objs Version=""1.1.0.1"" xmlns=""http://schemas.microsoft.com/powershell/2004/04"">
                    <S>Test String</S>
                </Objs>";
            A.CallTo(() => file.ReadAllText("c:\\temp\\{RANDOMGUID}.stdout")).Returns(xmldata);
            A.CallTo(() => file.Exists("c:\\temp\\{RANDOMGUID}.stdout")).Returns(true);

            var result = sut.ExecutePowershell("#MyScript", null);

            Assert.That(result.Result.ToString() == "Test String");
        }

        [Test]
        public void ExceutePowershell_STDErrReturned_ErrInResult()
        {
            var file = A.Fake<IFileWrap>();
            var env = DefaultEnvironmentFactory(getguid: "{RANDOMGUID}", tempdir: "c:\\temp");
            var svc = DefaultServiceDiscoveryFactory(file: file, env: env);
            var sut = DefaultShimFactory(svc: svc);
            const string xmldata = @"
                <Objs Version=""1.1.0.1"" xmlns=""http://schemas.microsoft.com/powershell/2004/04"">
                    <S>Test String</S>
                </Objs>";
            A.CallTo(() => file.ReadAllText("c:\\temp\\{RANDOMGUID}.stderr")).Returns(xmldata);
            A.CallTo(() => file.Exists("c:\\temp\\{RANDOMGUID}.stderr")).Returns(true);

            var result = sut.ExecutePowershell("#MyScript", null);

            Assert.That(result.Error.ToString() == "Test String");
        }

        [Test]
        public void ExceutePowershell_STDOutEmpty_NullReturned()
        {
            var file = A.Fake<IFileWrap>();
            var env = DefaultEnvironmentFactory(getguid: "{RANDOMGUID}", tempdir: "c:\\temp");
            var svc = DefaultServiceDiscoveryFactory(file: file, env: env);
            var sut = DefaultShimFactory(svc: svc);
            const string xmldata = "";

            A.CallTo(() => file.ReadAllText("c:\\temp\\{RANDOMGUID}.stdout")).Returns(xmldata);
            A.CallTo(() => file.Exists("c:\\temp\\{RANDOMGUID}.stdout")).Returns(true);

            var result = sut.ExecutePowershell("#MyScript", null);

            Assert.That(result.Result == null);
        }

        [Test]
        public void ExceutePowershell_STDErrEmpty_NullReturned()
        {
            var file = A.Fake<IFileWrap>();
            var env = DefaultEnvironmentFactory(getguid: "{RANDOMGUID}", tempdir: "c:\\temp");
            var svc = DefaultServiceDiscoveryFactory(file: file, env: env);
            var sut = DefaultShimFactory(svc: svc);
            const string xmldata = "";
            A.CallTo(() => file.ReadAllText("c:\\temp\\{RANDOMGUID}.stderr")).Returns(xmldata);
            A.CallTo(() => file.Exists("c:\\temp\\{RANDOMGUID}.stderr")).Returns(true);

            var result = sut.ExecutePowershell("#MyScript", null);

            Assert.That(result.Error == null);
        }

        [TestCase("c:\\temp\\{RANDOMGUID}.ps1" )]
        [TestCase("c:\\temp\\{RANDOMGUID}.cmd")]
        [TestCase("c:\\temp\\{RANDOMGUID}.data.xml")]
        [TestCase("c:\\temp\\{RANDOMGUID}.stdout")]
        [TestCase("c:\\temp\\{RANDOMGUID}.stderr")]
        public void ExecutePowershell_CleanUp_FilesCleanedOnLocalSystem(string filename)
        {
            var file = A.Fake<IFileWrap>();
            var env = DefaultEnvironmentFactory(getguid: "{RANDOMGUID}", tempdir: "c:\\temp");
            var svc = DefaultServiceDiscoveryFactory(file: file, env: env);
            var sut = DefaultShimFactory(svc: svc);
            A.CallTo(() => file.Exists(filename)).Returns(true);

            sut.ExecutePowershell("#scriptfile", null);

            A.CallTo(() => file.Delete(filename)).MustHaveHappened();
        }

        [TestCase("c:\\windows\\temp\\{RANDOMGUID}.ps1")]
        [TestCase("c:\\windows\\temp\\{RANDOMGUID}.cmd")]
        [TestCase("c:\\windows\\temp\\{RANDOMGUID}.data.xml")]
        [TestCase("c:\\windows\\temp\\{RANDOMGUID}.stdout")]
        [TestCase("c:\\windows\\temp\\{RANDOMGUID}.stderr")]
        public void ExceutePowershell_CleanUp_FilesOnGuestCleaned(string filename)
        {
            var subvm = A.Fake<IVirtualMachine>();
            var env = DefaultEnvironmentFactory(getguid: "{RANDOMGUID}", tempdir: "c:\\temp");
            var svc = DefaultServiceDiscoveryFactory(env: env);
            var sut = DefaultShimFactory(svc: svc, subvm: subvm);
            A.CallTo(() => subvm.FileExists(filename)).Returns(true);

            sut.ExecutePowershell("#scriptfile", null);

            A.CallTo(() => subvm.DeleteFile(filename)).MustHaveHappened();
        }

    }
}
