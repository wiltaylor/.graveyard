using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using FakeItEasy;
using NUnit.Framework;
using VixCOM;
using VMLib.Exceptions;
using VMLib.VMware.VIXItems;

namespace VMLib.VMware.UnitTest
{
    [TestFixture]
    public class VMwareVirtualMachineTests
    {
        public IVirtualMachine DefaultVMwareVirtualMachineFactory(IVix vix = null, string vmpath = "c:\\testvm.vmx", IVM2 vm = null)
        {
            if (vix == null)
                vix = A.Fake<IVix>();

            if (vm == null)
                vm = A.Fake<IVM2>();

            A.CallTo(() => vix.ConnectToVM(vmpath)).Returns(vm);

            var sut = new VMwareVirtualMachine(vmpath, vix);

            return sut;
        }

        [TestCase(VixPowerState.Off, VMState.Off)]
        [TestCase(VixPowerState.Pending, VMState.Pending)]
        [TestCase(VixPowerState.Suspended, VMState.Off)]
        [TestCase(VixPowerState.Ready, VMState.Ready)]
        public void State_VIXPowerState_ReturnsExpectedVMState(VixPowerState vixstate, VMState vmstate)
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(vixstate);

            var result = sut.State;

            Assert.That(result == vmstate);
        }

        [Test]
        public void Start_PowerOnVM_WillCallVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.Start();

            A.CallTo(() => vix.PowerOn(A<IVM2>.Ignored)).MustHaveHappened();
        }

        [TestCase(true)]
        [TestCase(false)]
        public void Stop_PowerOffVM_WillCallVix(bool force)
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.Stop(force);

            A.CallTo(() => vix.PowerOff(A<IVM2>.Ignored, force)).MustHaveHappened();
        }

        [TestCase(true)]
        [TestCase(false)]
        public void Restart_RestartVM_WillCallVix(bool force)
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.Restart(force);

            A.CallTo(() => vix.Restart(A<IVM2>.Ignored, force)).MustHaveHappened();
        }

        [Test]
        public void CreateSnapshot_CreatesASnapshotOfVM()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.CreateSnapshot("MySnapshot", "Description");

            A.CallTo(() => vix.CreateSnapshot(A<IVM2>.Ignored, "MySnapshot", "Description", A<bool>.Ignored))
                .MustHaveHappened();
        }

        [Test]
        public void CreateSNapshot_WhenPoweredOn_PassTrueToCaptureMemory()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Ready);

            sut.CreateSnapshot("anyname", "anydescription");

            A.CallTo(() => vix.CreateSnapshot(A<IVM2>.Ignored, A<string>.Ignored, A<string>.Ignored, true))
                .MustHaveHappened();
        }

        [Test]
        public void CreateSnapshot_WhenPoweredOff_PassFalseToCaptureMemory()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Off);

            sut.CreateSnapshot("anyname", "anydescription");

            A.CallTo(() => vix.CreateSnapshot(A<IVM2>.Ignored, A<string>.Ignored, A<string>.Ignored, false))
                .MustHaveHappened();
        }

        [Test]
        public void GetSnapshots_ReturnsListOfSnapshots_ReturnsStrings()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            var snapshot = A.Fake<ISnapshot>();
            A.CallTo(() => vix.GetSnapshots(A<IVM2>.Ignored)).Returns(new[] {snapshot});
            A.CallTo(() => vix.GetSnapshotName(snapshot)).Returns("MySnapshot");

            var result = sut.Snapshots.ToArray();

            Assert.That(result[0] == "MySnapshot");
        }

        [Test]
        public void RestoreSnapshot_RevertToNamedSnapshot_CallsVixToRevert()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            var snapshot = A.Fake<ISnapshot>();
            A.CallTo(() => vix.GetSnapshots(A<IVM2>.Ignored)).Returns(new[] { snapshot });
            A.CallTo(() => vix.GetSnapshotName(snapshot)).Returns("MySnapshot");

            sut.RestoreSnapshot("MySnapshot");

            A.CallTo(() => vix.RevertToSnapshot(A<IVM2>.Ignored, snapshot, A<bool>.Ignored)).MustHaveHappened();
        }

        [Test]
        public void RestoreSnapshot_RevertToNamedSnapshotTHatDoesntExist_Throws()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            Assert.Throws<SnapshotDoesntExistExceptipon>(() => sut.RestoreSnapshot("NonExistingSnapshot"));
        }

        [Test]
        public void RemoveSnapshot_RemoveExistingSnapshot_CallsVixToRemove()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            var snapshot = A.Fake<ISnapshot>();
            A.CallTo(() => vix.GetSnapshots(A<IVM2>.Ignored)).Returns(new[] { snapshot });
            A.CallTo(() => vix.GetSnapshotName(snapshot)).Returns("MySnapshot");

            sut.RemoveSnapshot("MySnapshot");

            A.CallTo(() => vix.RemoveSnapshot(A<IVM2>.Ignored, snapshot, false));
        }

        [Test]
        public void RemoveSnapshot_RemoveNonExistingSnapshot_Throws()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            Assert.Throws<SnapshotDoesntExistExceptipon>(() => sut.RemoveSnapshot("NonExistingSnapshot"));
        }

        [Test]
        public void AddSharedFolder_AddNewFolderToVM_CallsVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.AddSharedFolder("MyShare", "c:\\sharedcontent", true);

            A.CallTo(() => vix.AddSharedFolder(A<IVM2>.Ignored, "c:\\sharedcontent", "MyShare", true)).MustHaveHappened();
        }

        [Test]
        public void AddSharedFolder_EnablesSharedFolders_CallVixToEnableSharedFolders()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.AddSharedFolder("MyShare", "c:\\sharedcontent", true);

            A.CallTo(() => vix.EnableSharedFolders(A<IVM2>.Ignored)).MustHaveHappened();
        }

        [Test]
        public void AddSharedFolder_GuestOSMustLogonBeforeUse_CallToVixToLogin()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.AddSharedFolder("MyShare", "C:\\sharedcontent", true);

            A.CallTo(() => vix.LoginToGuest(A<IVM2>.Ignored, A<string>.Ignored, A<string>.Ignored, A<bool>.Ignored))
                .MustHaveHappened();
        }

        [Test]
        public void AddSharedFolder_WaitForTools_CallToVixToWait()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.AddSharedFolder("MyShare", "C:\\sharedcontent", true);

            A.CallTo(() => vix.WaitForTools(A<IVM2>.Ignored)).MustHaveHappened();
        }

        [Test]
        public void RemoveSharedFolder_RemoveSharedFolder_CallsVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.RemoveSharedFolder("MyShare");

            A.CallTo(() => vix.RemoveSharedFolder(A<IVM2>.Ignored, "MyShare")).MustHaveHappened();
        }

        [Test]
        public void ExecuteCommand_CanExecuteCommndAndWaitForIt_CallsVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.ExecuteCommand("c:\\myapp.exe", "-someargs", true, false);

            A.CallTo(() => vix.ExecuteCommand(A<IVM2>.Ignored, "c:\\myapp.exe", "-someargs", false, true))
                .MustHaveHappened();
        }

        [Test]
        public void ExecuteCommand_WillLogInToGuestOS_CallToVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            sut.Username = "username";
            sut.Password = "password";

            sut.ExecuteCommand("c:\\myapp.exe", "-someargs", true, false);

            A.CallTo(() => vix.LoginToGuest(A<IVM2>.Ignored, "username", "password", false)).MustHaveHappened();
        }

        [Test]
        public void ExecuteCommand_WillWaitForToolsToBeReady_CallToVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.ExecuteCommand("c:\\myapp.exe", "-someargs", true, false);

            A.CallTo(() => vix.WaitForTools(A<IVM2>.Ignored)).MustHaveHappened();
        }

        [Test]
        public void ExecuteCommand_CommandRunInteractivly_VixIsToldToLoginInteractivly()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            sut.Username = "username";
            sut.Password = "password";

            sut.ExecuteCommand("c:\\myapp.exe", "-someargs", true, true);

            A.CallTo(() => vix.LoginToGuest(A<IVM2>.Ignored, "username", "password", true)).MustHaveHappened();
        }

        [Test]
        public void ExecutePowershell_CommandNotSupported_WillThrow()
        {
            var sut = DefaultVMwareVirtualMachineFactory();

            Assert.Throws<UnsupportedVMFeature>(() => sut.ExecutePowershell("#Any powershell script"));
        }

        [Test]
        public void Processes_GetsListOfProcesses_CallToVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            var vixprocess = new VixProcess
            {
                Name = "MyProcess",
                Command = "c:\\myapp.exe",
                Owner = "Administrator",
                ProcessID = 1000
            };
            A.CallTo(() => vix.GetProcesses(A<IVM2>.Ignored)).Returns(new [] {vixprocess});

            var result = sut.Processes;

            Assert.That(result.FirstOrDefault().ProcessID == 1000 );
        }

        [Test]
        public void KillProcess_KillProcessWithTargetPID_CallsVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.KillProcess(1000);

            A.CallTo(() => vix.KillProcess(A<IVM2>.Ignored, 1000)).MustHaveHappened();
        }

        [Test]
        public void KillProcess_GuestLogin_LoginWillBeCalledBeforeKillingProcess()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.KillProcess(1000);

            A.CallTo(() => vix.LoginToGuest(A<IVM2>.Ignored, A<string>.Ignored, A<string>.Ignored, A<bool>.Ignored))
                .MustHaveHappened();
        }

        [Test]
        public void KillProcess_WaitForToolsToBeReady_CallToVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.KillProcess(1000);

            A.CallTo(() => vix.WaitForTools(A<IVM2>.Ignored)).MustHaveHappened();
        }

        [Test]
        public void CopyToVM_CopyingFileIntoVM_CallToVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.CopyToVM("c:\\hostpath\\test.txt", "c:\\guestpath\\test.txt");

            A.CallTo(() => vix.CopyFileToGuest(A<IVM2>.Ignored, "c:\\hostpath\\test.txt", "c:\\guestpath\\test.txt"))
                .MustHaveHappened();
        }

        [Test]
        public void CopyToVM_LoginToGuestOS_CallToVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.CopyToVM("c:\\hostpath\\test.txt", "c:\\guestpath\\test.txt");

            A.CallTo(() => vix.LoginToGuest(A<IVM2>.Ignored, A<string>.Ignored, A<string>.Ignored, A<bool>.Ignored))
                .MustHaveHappened();
        }

        [Test]
        public void CopyToVM_WaitForToolsToBeReady_CallToVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.CopyToVM("c:\\hostpath\\test.txt", "c:\\guestpath\\test.txt");

            A.CallTo(() => vix.WaitForTools(A<IVM2>.Ignored)).MustHaveHappened();
        }

        [Test]
        public void CopyFromVM_CopyingFileFromVMToHost_CallToVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.CopyFromVM("c:\\guestpath\\test.txt", "c:\\hostpath\\test.txt");

            A.CallTo(() => vix.CopyFileToHost(A<IVM2>.Ignored, "c:\\guestpath\\test.txt", "c:\\hostpath\\test.txt"))
                .MustHaveHappened();
        }

        [Test]
        public void CopyFromVM_LoginToGuestOS_CallToVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.CopyFromVM("c:\\guestpath\\test.txt", "c:\\hostpath\\test.txt");

            A.CallTo(() => vix.LoginToGuest(A<IVM2>.Ignored, A<string>.Ignored, A<string>.Ignored, A<bool>.Ignored))
                .MustHaveHappened();
        }

        [Test]
        public void CopyFromVM_WaitForTools_CallToVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.CopyFromVM("c:\\guestpath\\test.txt", "c:\\hostpath\\test.txt");

            A.CallTo(() => vix.WaitForTools(A<IVM2>.Ignored)).MustHaveHappened();
        }

        [TestCase(true)]
        [TestCase(false)]
        public void FileExists_TestIfFileExistInVM_CallToVix(bool existresult)
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            A.CallTo(() => vix.FileExists(A<IVM2>.Ignored, "c:\\myfile.txt")).Returns(existresult);

            var result = sut.FileExists("c:\\myfile.txt");

            Assert.That(result == existresult);
        }

        [Test]
        public void FileExists_LogsinToGuestOS_CallToVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            A.CallTo(() => vix.FileExists(A<IVM2>.Ignored, "c:\\myfile.txt")).Returns(false);

            var result = sut.FileExists("c:\\myfile.txt");

            A.CallTo(() => vix.LoginToGuest(A<IVM2>.Ignored, A<string>.Ignored, A<string>.Ignored, A<bool>.Ignored))
    .MustHaveHappened();
        }

        [Test]
        public void FileExists_WaitForTools_CallToVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            A.CallTo(() => vix.FileExists(A<IVM2>.Ignored, "c:\\myfile.txt")).Returns(false);

            var result = sut.FileExists("c:\\myfile.txt");

            A.CallTo(() => vix.WaitForTools(A<IVM2>.Ignored)).MustHaveHappened();
        }

        [TestCase(true)]
        [TestCase(false)]
        public void DirectoryExists_TestIfDirectoryExistsInVM_CallToVix(bool existresult)
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            A.CallTo(() => vix.DirectoryExist(A<IVM2>.Ignored, "c:\\myfolder")).Returns(existresult);

            var result = sut.DirectoryExists("c:\\myfolder");

            Assert.That(result == existresult);
        }

        [Test]
        public void DirectoryExists_LoginToGuestOS_CallToVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            A.CallTo(() => vix.DirectoryExist(A<IVM2>.Ignored, "c:\\myfolder")).Returns(false);

            var result = sut.DirectoryExists("c:\\myfolder");

            A.CallTo(() => vix.LoginToGuest(A<IVM2>.Ignored, A<string>.Ignored, A<string>.Ignored, A<bool>.Ignored))
                .MustHaveHappened();
        }

        [Test]
        public void DirectoryExists_WaitOnTools_CallToVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            A.CallTo(() => vix.DirectoryExist(A<IVM2>.Ignored, "c:\\myfolder")).Returns(false);

            var result = sut.DirectoryExists("c:\\myfolder");

            A.CallTo(() => vix.WaitForTools(A<IVM2>.Ignored)).MustHaveHappened();
        }

        [Test]
        public void DeleteFile_RemoveFileFromGuestOS_CallToVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.DeleteFile("c:\\myfile.txt");

            A.CallTo(() => vix.DeleteFileInGuest(A<IVM2>.Ignored, "c:\\myfile.txt")).MustHaveHappened();
        }

        [Test]
        public void DeleteFile_LoginToGuestOS_CallToVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.DeleteFile("c:\\myfile.txt");

            A.CallTo(() => vix.LoginToGuest(A<IVM2>.Ignored, A<string>.Ignored, A<string>.Ignored, A<bool>.Ignored))
                .MustHaveHappened();
        }

        [Test]
        public void DeleteFile_WaitForTools_CallToVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.DeleteFile("c:\\myfile.txt");

            A.CallTo(() => vix.WaitForTools(A<IVM2>.Ignored)).MustHaveHappened();
        }

        [Test]
        public void DeleteDirectory_DeleteFolderFromGuestOS_CallToVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.DeleteDirectory("c:\\myfolder");

            A.CallTo(() => vix.DeleteDirectoryInGuest(A<IVM2>.Ignored, "c:\\myfolder")).MustHaveHappened();
        }

        [Test]
        public void DeleteDirectory_LoginToGuestOS_CallToVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.DeleteDirectory("c:\\myfolder");

            A.CallTo(() => vix.LoginToGuest(A<IVM2>.Ignored, A<string>.Ignored, A<string>.Ignored, A<bool>.Ignored))
                .MustHaveHappened();
        }

        [Test]
        public void DeleteDirectory_WaitForTools_CallToVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.DeleteDirectory("c:\\myfolder");

            A.CallTo(() => vix.WaitForTools(A<IVM2>.Ignored)).MustHaveHappened();
        }

        [Test]
        public void WaitTillReady_WaitForVMToBecomeReady_CallToVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.WaitTillReady();

            A.CallTo(() => vix.WaitForTools(A<IVM2>.Ignored)).MustHaveHappened();
        }



    }
}
