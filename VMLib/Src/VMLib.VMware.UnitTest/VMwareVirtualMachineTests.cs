using System;
using System.Collections;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using SystemWrapper.IO;
using SystemWrapper.Threading;
using FakeItEasy;
using NUnit.Framework;
using VixCOM;
using VMLib.Exceptions;
using VMLib.IOC;
using VMLib.UnitTest;
using VMLib.VMware.Exceptions;
using VMLib.VMware.VIXItems;

namespace VMLib.VMware.UnitTest
{
    [TestFixture]
    public class VMwareVirtualMachineTests
    {
        public IVirtualMachine DefaultVMwareVirtualMachineFactory(IVix vix = null, string vmpath = "c:\\testvm.vmx", IVM2 vm = null, IServiceDiscovery srvDiscovery = null, IVMXHelper vmx = null, IHypervisorConnectionInfo info = null, IFileWrap file = null)
        {
            if (vix == null)
                vix = A.Fake<IVix>();

            if (vm == null)
                vm = A.Fake<IVM2>();

            if (vmx == null)
                vmx = A.Fake<IVMXHelper>();

            if (srvDiscovery == null)
                srvDiscovery = FakeServiceDiscovery.ReturnTestableInstance();

            if (info == null)
                info = A.Fake<IHypervisorConnectionInfo>();

            if (file == null)
                file = A.Fake<IFileWrap>();

            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Pending);

            A.CallTo(() => vix.ConnectToVM(vmpath)).Returns(vm);

            var sut = new VMwareVirtualMachine(vmpath, vix, vmx, info, file);

            return sut;
        }

        public IVMNetwork DefaultVMwareNetwork(VMNetworkType type = VMNetworkType.Bridged, string macAddress = "00:00:00:00:00:00:00:00", IDictionary<string, string> customsettings = null)
        {
            if(customsettings == null)
                customsettings = new Dictionary<string, string>();

            if(!customsettings.ContainsKey("DeviceType"))
                customsettings.Add("DeviceType", "E1000");

            var fake = A.Fake<IVMNetwork>();
            A.CallTo(() => fake.Type).Returns(type);
            A.CallTo(() => fake.MACAddress).Returns(macAddress);
            A.CallTo(() => fake.CustomSettings).Returns(customsettings);
            return fake;

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

            Assert.Throws<UnsupportedVMFeature>(() => sut.ExecutePowershell("#Any powershell script", null));
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

            Assert.That(result.First().ProcessID == 1000);
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

        [Test]
        public void WaitTillReady_TimeOutExceptionThrow_WillKeepWaiting()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            A.CallTo(() => vix.WaitForTools(A<IVM2>.Ignored))
                .Throws(new VixException("Time out exception", Constants.VIX_E_TIMEOUT_WAITING_FOR_TOOLS)).Twice();

            Assert.DoesNotThrow(() => sut.WaitTillReady());
        }

        [Test]
        public void WaitTillReady_AnyOtherExceptionOtherThanTimeOut_WillThrow()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            A.CallTo(() => vix.WaitForTools(A<IVM2>.Ignored))
                .Throws(new ApplicationException("AnyOtherException"));

            Assert.Throws<ApplicationException>(() => sut.WaitTillReady());
        }

        [Test]
        public void WaitTillReady_VMPoweredOff_WillThrow()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Off);

            Assert.Throws<VMPoweredOffException>(() => sut.WaitTillReady());
        }

        [Test]
        public void WaitTillOff_WillWaitForPowerOff_WillReturn()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Off).Once(); //note: don't forget last in first out on call stack for fakeiteasy.
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Pending).Once();
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Ready).Once();

            sut.WaitTillOff();

            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).MustHaveHappened(Repeated.Like(i => i == 3));
        }

        [Test]
        public void WaitTillOff_WillWaitASecondBetweenEachCall_CallToThreadSleepL()
        {
            var srvDiscovery = FakeServiceDiscovery.ReturnTestableInstance();
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix, srvDiscovery: srvDiscovery);
            var thread = A.Fake<IThreadWrap>();
            A.CallTo(() => srvDiscovery.Resolve<IThreadWrap>()).Returns(thread);
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Off).Once();
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Ready).Once();

            sut.WaitTillOff();

            A.CallTo(() => thread.Sleep(1000)).MustHaveHappened();
        }

        [Test]
        public void ReadEnvironment_WillRetriveEnvironmentFromOS_CallToVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            A.CallTo(() => vix.ReadVariable(A<IVM2>.Ignored, "MyVar", VixVariable.Environment)).Returns("MyValue");
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Ready);

            var result = sut.ReadEnvironment("MyVar");

            Assert.That(result == "MyValue");
        }

        [Test]
        public void ReadEnvironment_WillThrowIfVMPoweredOff_CallToVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Off);

            Assert.Throws<VMPoweredOffException>(() => sut.ReadEnvironment("MyVar"));
        }

        [Test]
        public void ReadEnvironment_WillLoginToGuestOS_CallToVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Ready);
            
            sut.ReadEnvironment("MyVar");

            A.CallTo(() => vix.LoginToGuest(A<IVM2>.Ignored, A<string>.Ignored, A<string>.Ignored, A<bool>.Ignored))
                .MustHaveHappened();
        }

        [Test]
        public void ReadGuestVariable_WillReadVariableFromVM_CallToVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Ready);
            A.CallTo(() => vix.ReadVariable(A<IVM2>.Ignored, "MyVar", VixVariable.GuestVar)).Returns("MyValue");

            var result = sut.ReadGuestVariable("MyVar");

            Assert.That(result == "MyValue");
        }

        [Test]
        public void ReadGuestVariable_VixThrowsException_NullReturned()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            A.CallTo(() => vix.ReadVariable(A<IVM2>.Ignored, "MyVar", VixVariable.GuestVar))
                .Throws(new Exception("Any exception"));

            var result = sut.ReadGuestVariable("MyVar");

            Assert.IsNull(result);
        }

        [Test]
        public void ReadGuestVariable_WhileVMPoweredOff_WillReturnValueFromVmxReader()
        {
            var vmx = A.Fake<IVMXHelper>();
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vmx: vmx, vix: vix);
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Off);
            A.CallTo(() => vmx.ReadVMX("guestinfo.MyVar")).Returns("MyValue");

            var result = sut.ReadGuestVariable("MyVar");

            Assert.That(result == "MyValue");
        }

        [Test]
        public void ReadVMSetting_ReadingVMXFilePoweredOn_WillReadFromVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            A.CallTo(() => vix.ReadVariable(A<IVM2>.Ignored, "myvmx.property", VixVariable.VMX)).Returns("MyValue");

            var result = sut.ReadVMSetting("myvmx.property");

            Assert.That(result == "MyValue");
        }

        [Test]
        public void ReadVMSetting_ReadingVMXFilePoweredOff_WillReadfromVMXFile()
        {
            var vix = A.Fake<IVix>();
            var vmx = A.Fake<IVMXHelper>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix, vmx: vmx);
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Off);
            A.CallTo(() => vmx.ReadVMX("myvmx.property")).Returns("MyValue");

            var result = sut.ReadVMSetting("myvmx.property");

            Assert.That(result == "MyValue");
        }

        [Test]
        public void WriteEnvironment_WhenGuestIsOn_WillCallVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.WriteEnvironment("MyVar", "MyValue");

            A.CallTo(() => vix.WriteVariable(A<IVM2>.Ignored, "MyVar", "MyValue", VixVariable.Environment)).MustHaveHappened();
        }

        [Test]
        public void WriteEnvironment_WhenGuestIsOff_WillThrow()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Off);

            Assert.Throws<VMPoweredOffException>(() => sut.WriteEnvironment("MyVar", "MyValue"));
        }

        [Test]
        public void WriteEnvironment_LogsInToGuestOS_CallsVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.WriteEnvironment("MyVar", "MyValue");

            A.CallTo(() => vix.LoginToGuest(A<IVM2>.Ignored, A<string>.Ignored, A<string>.Ignored, A<bool>.Ignored))
                .MustHaveHappened();
        }

        [Test]
        public void WriteGuestVariable_WhenGuestIsOn_CallsVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.WriteGuestVariable("MySetting", "MyValue");

            A.CallTo(() => vix.WriteVariable(A<IVM2>.Ignored, "MySetting", "MyValue", VixVariable.GuestVar))
                .MustHaveHappened();
        }

        [Test]
        public void WriteGuestVariable_WhenGuestIsOff_CallVMXHelper()
        {
            var vix = A.Fake<IVix>();
            var vmx = A.Fake<IVMXHelper>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix, vmx: vmx);
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Off);

            sut.WriteGuestVariable("MySetting", "MyValue");

            A.CallTo(() => vmx.WriteVMX("guestinfo.MySetting", "MyValue")).MustHaveHappened();
        }

        [Test]
        public void WriteGuestVariable_WhenGuestIsOff_WillWriteToDisk()
        {
            var vix = A.Fake<IVix>();
            var vmx = A.Fake<IVMXHelper>();
            var file = A.Fake<IFileWrap>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix, vmx: vmx, file: file, vmpath: "c:\\myvm.vmx");
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Off);
            A.CallTo(() => vmx.ToArray()).Returns(new [] { "VMXData"});
            
            sut.WriteGuestVariable("MySetting", "MyValue");

            A.CallTo(() => file.WriteAllLines("c:\\myvm.vmx", A<string[]>.Ignored)).MustHaveHappened();
        }

        [Test]
        public void WriteVMSetting_WhenGuestIsOn_CallsVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Ready);

            sut.WriteVMSetting("MySetting.Property", "MyValue");

            A.CallTo(() => vix.WriteVariable(A<IVM2>.Ignored, "MySetting.Property", "MyValue", VixVariable.VMX))
                .MustHaveHappened();
        }

        [Test]
        public void WriteVMSetting_WhenGuestIsOff_CallsVmxHelper()
        {
            var vix = A.Fake<IVix>();
            var vmx = A.Fake<IVMXHelper>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix, vmx: vmx);
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Off);

            sut.WriteVMSetting("MySetting.Property", "MyValue");

            A.CallTo(() => vmx.WriteVMX("MySetting.Property", "MyValue")).MustHaveHappened();
        }

        [Test]
        public void WriteVMSetting_WhenGuestIsOff_WritesToDisk()
        {
            var vix = A.Fake<IVix>();
            var vmx = A.Fake<IVMXHelper>();
            var file = A.Fake<IFileWrap>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix, vmx: vmx, vmpath: "c:\\myvm.vmx", file: file);
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Off);
            A.CallTo(() => vmx.ToArray()).Returns(new [] { "VMXData"});

            sut.WriteVMSetting("MySetting.Property", "MyValue");

            A.CallTo(() => file.WriteAllLines("c:\\myvm.vmx", A<string[]>.Ignored)).MustHaveHappened();
        }

        [Test]
        public void AddNetworkCard_AddingNewCardToVM_WillcallVMXHelper()
        {
            var vix = A.Fake<IVix>();
            var vmx = A.Fake<IVMXHelper>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix, vmx: vmx);
            var network = DefaultVMwareNetwork();
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Off);

            sut.AddNetworkCard(network);

            A.CallTo(() => vmx.WriteNetwork(network)).MustHaveHappened();
        }

        [Test]
        public void AddNetworkCard_AddingNewCardToVM_WriteToDisk()
        {
            var vix = A.Fake<IVix>();
            var vmx = A.Fake<IVMXHelper>();
            var file = A.Fake<IFileWrap>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix, vmx: vmx, vmpath: "c:\\myvm.vmx", file: file);
            var network = DefaultVMwareNetwork();
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Off);
            A.CallTo(() => vmx.ToArray()).Returns(new[] { "VMXData" });

            sut.AddNetworkCard(network);

            A.CallTo(() => file.WriteAllLines("c:\\myvm.vmx", A<string[]>.Ignored)).MustHaveHappened();
        }

        [Test]
        public void AddNetworkCard_WhileVMIsPoweredOn_WillThrow()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Ready);
            var network = DefaultVMwareNetwork();

            Assert.Throws<VMPoweredOnException>(() => sut.AddNetworkCard(network));
        }

        [Test]
        public void GetNetworkCard_CallingMethod_WillReturnDataFromVMXHelper()
        {

            var vmx = A.Fake<IVMXHelper>();
            var sut = DefaultVMwareVirtualMachineFactory(vmx: vmx);
            var network = DefaultVMwareNetwork();
            A.CallTo(() => vmx.ReadNetwork()).Returns(new[] {network});

            var result = sut.GetNetworkCards();

            Assert.AreSame(network, result.First());
        }

        [Test]
        public void RemoveNetworkCard_WhileVMPoweredOff_WillPassRequestTOVMXHelper()
        {
            var vix = A.Fake<IVix>();
            var vmx = A.Fake<IVMXHelper>();
            var sut = DefaultVMwareVirtualMachineFactory(vmx: vmx, vix: vix);
            var network = DefaultVMwareNetwork();
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Off);

            sut.RemoveNetworkCard(network);

            A.CallTo(() => vmx.RemoveNetwork(network)).MustHaveHappened();
        }

        [Test]
        public void RemoveNetworkCard_WhileVMPoweredOff_WillWriteToDisk()
        {
            var vix = A.Fake<IVix>();
            var vmx = A.Fake<IVMXHelper>();
            var file = A.Fake<IFileWrap>();
            var sut = DefaultVMwareVirtualMachineFactory(vmx: vmx, vix: vix, vmpath: "c:\\myvm.vmx", file: file);
            var network = DefaultVMwareNetwork();
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Off);
            A.CallTo(() => vmx.ToArray()).Returns(new[] { "VMXData" });

            sut.RemoveNetworkCard(network);

            A.CallTo(() => file.WriteAllLines("c:\\myvm.vmx", A<string[]>.Ignored)).MustHaveHappened();
        }

        [Test]
        public void RemoveNetworkCard_WhileVMPoweredOn_WillThrow()
        {
            var vix = A.Fake<IVix>();
            var vmx = A.Fake<IVMXHelper>();
            var sut = DefaultVMwareVirtualMachineFactory(vmx: vmx, vix: vix);
            var network = DefaultVMwareNetwork();
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Ready);

            Assert.Throws<VMPoweredOnException>(() => sut.RemoveNetworkCard(network));
        }


        [Test]
        public void AddDisk_WhileVMIsOff_WillCallVMXHelper()
        {
            var vmx = A.Fake<IVMXHelper>();
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix, vmx: vmx);
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Off);

            sut.AddDisk(disk);

            A.CallTo(() => vmx.WriteDisk(disk)).MustHaveHappened();
        }

        [Test]
        public void AddDisk_WhileVMIsOff_WillWriteToDisk()
        {
            var vmx = A.Fake<IVMXHelper>();
            var vix = A.Fake<IVix>();
            var file = A.Fake<IFileWrap>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix, vmx: vmx, vmpath: "c:\\myvm.vmx", file: file);
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Off);
            A.CallTo(() => vmx.ToArray()).Returns(new[] { "VMXData" });

            sut.AddDisk(disk);

            A.CallTo(() => file.WriteAllLines("c:\\myvm.vmx", A<string[]>.Ignored)).MustHaveHappened();
        }

        [Test]
        public void AddDisk_WhileVMIsOn_WillThrow()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Ready);

            Assert.Throws<VMPoweredOnException>(() => sut.AddDisk(disk));
        }

        [Test]
        public void GetDisk_WillReturnListOfDisks_WillCallVMXHelper()
        {
            var vmx = A.Fake<IVMXHelper>();
            var sut = DefaultVMwareVirtualMachineFactory(vmx: vmx);
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => vmx.ReadDisk()).Returns(new[] {disk});

            var result = sut.GetDisks();

            Assert.AreSame(disk, result.First());
        }

        [Test]
        public void RemoveDisk_WhenVMIsOff_WillCallVMXHelper()
        {
            var vmx = A.Fake<IVMXHelper>();
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix, vmx: vmx);
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Off);

            sut.RemoveDisk(disk);

            A.CallTo(() => vmx.RemoveDisk(disk)).MustHaveHappened();
        }

        [Test]
        public void RemoveDisk_WhenVMIsOff_WillWriteToDisk()
        {
            var vmx = A.Fake<IVMXHelper>();
            var vix = A.Fake<IVix>();
            var file = A.Fake<IFileWrap>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix, vmx: vmx, vmpath: "c:\\myvm.vmx", file: file);
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Off);
            A.CallTo(() => vmx.ToArray()).Returns(new[] { "VMXData" });

            sut.RemoveDisk(disk);

            A.CallTo(() => file.WriteAllLines("c:\\myvm.vmx", A<string[]>.Ignored)).MustHaveHappened();
        }

        [Test]
        public void RemoveDisk_WhenVMIsOn_WillCallVMXHelper()
        {
            var vmx = A.Fake<IVMXHelper>();
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix, vmx: vmx);
            var disk = A.Fake<IVMDisk>();
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Ready);

            Assert.Throws<VMPoweredOnException>(() => sut.RemoveDisk(disk));
        }

        [Test]
        public void OpenLocalGUI_MethodCalled_WillCallVix()
        {
            var vix = A.Fake<IVix>();
            var info = A.Fake<IHypervisorConnectionInfo>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix, vmpath: "c:\\myvm\\myvm.vmx", info: info);
            A.CallTo(() => info.Properties).Returns(new Dictionary<string, string> { {"VMwareWorkstationPath", "C:\\Program Files (x86)\\VMware\\VMware Workstation\\"}});

            sut.OpenLocalGUI();

            A.CallTo(() => vix.OpenLocalUI("c:\\myvm\\myvm.vmx", "C:\\Program Files (x86)\\VMware\\VMware Workstation\\")).MustHaveHappened();
        }

        [Test]
        public void RemoteAccessProtocol_DefaultValue_SetToNone()
        {
            var sut = DefaultVMwareVirtualMachineFactory();

            var result = sut.RemoteAccessProtocol;

            Assert.That(result == RemoteProtocol.None);
        }

        [Test]
        public void CreateRemoteConnection_CallingWithPort_WillChangeConnectionRemoteAccessPortocolToVNC()
        {
            var sut = DefaultVMwareVirtualMachineFactory();

            sut.CreateRemoteConnection(1337, "Password");

            Assert.That(sut.RemoteAccessProtocol == RemoteProtocol.VNC);
        }

        [Test]
        public void CreateRemoteConnection_CallingWithPort_WillStorePortInRemoteAccessPort()
        {
            var sut = DefaultVMwareVirtualMachineFactory();

            sut.CreateRemoteConnection(1337, "Password");

            Assert.That(sut.RemoteAccessPort == 1337);
        }

        [Test]
        public void CreateRemoteConnection_CallingWithPassword_WillStorePasswordInRemoteAccessPassword()
        {
            var sut = DefaultVMwareVirtualMachineFactory();

            sut.CreateRemoteConnection(1337, "Password");

            Assert.That(sut.RemoteAccessPassword == "Password");
        }

        [TestCase("RemoteDisplay.vnc.enabled", "TRUE")]
        [TestCase("RemoteDisplay.vnc.password", "Password")]
        [TestCase("RemoteDisplay.vnc.port", "1337")]
        public void CreateRemoteConnection_CallingWithPortWhileGuestOn_CallVix(string setting, string value)
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Ready);
            
            sut.CreateRemoteConnection(1337, "Password");

            A.CallTo(() => vix.WriteVariable(A<IVM2>.Ignored, setting, value, VixVariable.VMX)).MustHaveHappened();
        }

        [TestCase("RemoteDisplay.vnc.enabled", "TRUE")]
        [TestCase("RemoteDisplay.vnc.password", "Password")]
        [TestCase("RemoteDisplay.vnc.port", "1337")]
        public void CreateRemoteConnection_CallingWithPortWhileGuestOff_WillCallVMXHelper(string setting, string value)
        {
            var vix = A.Fake<IVix>();
            var vmx = A.Fake<IVMXHelper>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix, vmx: vmx);
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Off);
            
            sut.CreateRemoteConnection(1337, "Password");

            A.CallTo(() => vmx.WriteVMX(setting, value)).MustHaveHappened();
        }

        [TestCase(0)]
        [TestCase(-1)]
        [TestCase(1023)]
        [TestCase(49152)] //Start Emphemeral ports.
        public void CreateRemoteConnection_CallingWithInvalidPorts_WillThrow(int port)
        {
            var sut = DefaultVMwareVirtualMachineFactory();

            Assert.Throws<InvalidRemoteConnectionPropertiesException>(() => sut.CreateRemoteConnection(port, "Password"));
        }

        [TestCase]
        public void CreateRemoteConnection_CallingWithInvalidPassword_WillThrow()
        {
            var sut = DefaultVMwareVirtualMachineFactory();

            Assert.Throws<InvalidRemoteConnectionPropertiesException>(() => sut.CreateRemoteConnection(1337, null));
            Assert.Throws<InvalidRemoteConnectionPropertiesException>(() => sut.CreateRemoteConnection(1337, string.Empty));
        }

        [Test]
        public void RemoveRemoteConnection_WhileGuestOn_WillCallVixToClearIt()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Ready);

            sut.RemoveRemoteConnection();

            A.CallTo(() => vix.WriteVariable(A<IVM2>.Ignored, "RemoteDisplay.vnc.enabled", "FALSE", VixVariable.VMX))
                .MustHaveHappened();
        }

        [Test]
        public void RemoveRemoteConnection_WhileGuestOff_WillCallVMXWriterToClearIt()
        {
            var vix = A.Fake<IVix>();
            var vmx = A.Fake<IVMXHelper>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix, vmx: vmx);
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Off);

            sut.RemoveRemoteConnection();

            A.CallTo(() => vmx.WriteVMX("RemoteDisplay.vnc.enabled", "FALSE")).MustHaveHappened();
        }

        [Test]
        public void RemoveRemoteConnection_AllStates_WillClearRemoteProtocol()
        {
            var sut = DefaultVMwareVirtualMachineFactory();
            sut.CreateRemoteConnection(1337, "Password");

            sut.RemoveRemoteConnection();

            Assert.That(sut.RemoteAccessProtocol == RemoteProtocol.None);
        }


        [Test]
        public void RemoveRemoteConnection_AllStates_WillClearRemotePort()
        {
            var sut = DefaultVMwareVirtualMachineFactory();
            sut.CreateRemoteConnection(1337, "Password");

            sut.RemoveRemoteConnection();

            Assert.That(sut.RemoteAccessPort == 0);
        }

        [Test]
        public void RemoveRemoteConnection_AllStates_WillClearRemotePassword()
        {
            var sut = DefaultVMwareVirtualMachineFactory();
            sut.CreateRemoteConnection(1337, "Password");

            sut.RemoveRemoteConnection();

            Assert.That(sut.RemoteAccessPassword == null);
        }

        [Test]
        public void Constructor_WhenPassingInVMPath_WillGetPathPropertySet()
        {
            var sut = DefaultVMwareVirtualMachineFactory(vmpath: "c:\\testvm\\myvm.vmx");
            
            Assert.That(sut.VMPath == "c:\\testvm\\myvm.vmx");
        }

        [Test]
        public void Constructor_WhenPassingInVMPath_WillGetVMDirectoryPropertySet()
        {
            var sut = DefaultVMwareVirtualMachineFactory(vmpath: "c:\\testvm\\myvm.vmx");

            Assert.That(sut.VMDirectory == "c:\\testvm");
        }

        [Test]
        public void Clone_CallingCloneWithLinkedSetFalse_WillCallVIX()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            
            sut.Clone("c:\\cloned\\vm.vmx", "MySnapshot", false);

            A.CallTo(() => vix.Clone("c:\\cloned\\vm.vmx", A<IVM2>.Ignored, A<ISnapshot>.Ignored, false))
                .MustHaveHappened();
        }

        [Test]
        public void Clone_CallingCloneWithLinkedSetTrue_WillCallVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.Clone("c:\\cloned\\vm.vmx", "MySnapshot", true);

            A.CallTo(() => vix.Clone("c:\\cloned\\vm.vmx", A<IVM2>.Ignored, A<ISnapshot>.Ignored, true))
                .MustHaveHappened();
        }

        [Test]
        public void Name_RetrivingVMName_WillReturnNameFromVMX()
        {
            var vmx = A.Fake<IVMXHelper>();
            var sut = DefaultVMwareVirtualMachineFactory(vmx: vmx);
            A.CallTo(() => vmx.ReadVMX("displayName")).Returns("MyVMName");

            Assert.That(sut.Name == "MyVMName");
        }

        [Test]
        public void Name_SettingVMName_WillCallVMX()
        {
            var vmx = A.Fake<IVMXHelper>();
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vmx: vmx, vix: vix);
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Off);

            sut.Name = "MyVM";

            A.CallTo(() => vmx.WriteVMX("displayName", "MyVM")).MustHaveHappened();
        }

        [Test]
        public void DeleteVM_PowerOff_WillCallVixToRemoveVM()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Off);

            sut.DeleteVM();

            A.CallTo(() => vix.Delete(A<IVM2>.Ignored)).MustHaveHappened();
        }

        [Test]
        public void DeleteVM_PoweredOn_WillStopVMFirst()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            A.CallTo(() => vix.GetState(A<IVM2>.Ignored)).Returns(VixPowerState.Ready);

            sut.DeleteVM();

            A.CallTo(() => vix.PowerOff(A<IVM2>.Ignored, true)).MustHaveHappened();
        }

        [Test]
        public void RenameFolder_NotSupported_WillThrow()
        {
            var sut = DefaultVMwareVirtualMachineFactory();

            Assert.Throws<UnsupportedVMFeature>(() => sut.RenameDirectory("C:\\anyfolder", "c:\\newfoldername"));
        }

        [Test]
        public void RenameFile_ExistingFileInVM_WillCallVix()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.RenameFile("c:\\myfile.txt", "c:\\newfilename.txt");

            A.CallTo(() => vix.RenameFileInGuest(A<IVM2>.Ignored, "c:\\myfile.txt", "c:\\newfilename.txt"))
                .MustHaveHappened();
        }

        [Test]
        public void RenameFile_NeedsToBeReady_WillCallVixToWaitTillToolsReady()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);

            sut.RenameFile("c:\\myfile.txt", "c:\\newfilename.txt");

            A.CallTo(() => vix.WaitForTools(A<IVM2>.Ignored)).MustHaveHappened();

        }

        [Test]
        public void RenameFile_NeedsToBeLoggedIn_WillCallVixToLogin()
        {
            var vix = A.Fake<IVix>();
            var sut = DefaultVMwareVirtualMachineFactory(vix: vix);
            sut.Username = "Username";
            sut.Password = "Password";

            sut.RenameFile("c:\\myfile.txt", "c:\\newfilename.txt");

            A.CallTo(() => vix.LoginToGuest(A<IVM2>.Ignored, "Username", "Password", false)).MustHaveHappened();
        }

    }
}
