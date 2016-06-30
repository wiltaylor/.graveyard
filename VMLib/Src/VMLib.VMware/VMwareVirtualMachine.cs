using System.Collections.Generic;
using System.Linq;
using SystemWrapper.IO;
using SystemWrapper.Threading;
using VixCOM;
using VMLib.Exceptions;
using VMLib.IOC;
using VMLib.VM;
using VMLib.VMware.Exceptions;
using VMLib.VMware.VIXItems;

namespace VMLib.VMware
{
    public class VMwareVirtualMachine : IVirtualMachine
    {
        private readonly string _vmPath;
        private readonly IVix _vix;
        private readonly IVM2 _vm;
        private readonly IVMXHelper _vmx;
        private readonly IHypervisorConnectionInfo _hypervisorInfo;
        private readonly IFileWrap _file;

        public VMwareVirtualMachine(string vmPath, IVix vix, IVMXHelper vmx, IHypervisorConnectionInfo hypervisorInfo, IFileWrap file)
        {
            _vmPath = vmPath;
            _vix = vix;
            _vmx = vmx;
            _hypervisorInfo = hypervisorInfo;
            _vm = vix.ConnectToVM(vmPath);
            RemoteAccessProtocol = RemoteProtocol.None;
            _file = file;
        }


        public VMState State
        {
            get
            {
                switch (_vix.GetState(_vm))
                {
                    case VixPowerState.Off:
                        return VMState.Off;
                    case VixPowerState.Ready:
                        return VMState.Ready;
                    case VixPowerState.Suspended:
                        return VMState.Off;
                    case VixPowerState.Pending:
                        break;
                }

                return VMState.Pending;
            }
        }

        public IEnumerable<string> Snapshots =>
            from s in _vix.GetSnapshots(_vm)
            select _vix.GetSnapshotName(s);

        public void Start()
        {
            _vix.PowerOn(_vm);
        }

        public void Stop(bool force)
        {
            _vix.PowerOff(_vm, force);
        }

        public void Restart(bool force)
        {
            _vix.Restart(_vm, force);
        }

        public void CreateSnapshot(string name, string description)
        {
            _vix.CreateSnapshot(_vm, name, description, State != VMState.Off);
        }

        public void RestoreSnapshot(string name)
        {
            var snapshot = _vix.GetSnapshots(_vm).FirstOrDefault(s => _vix.GetSnapshotName(s) == name);

            if (snapshot == null)
                throw new SnapshotDoesntExistExceptipon($"Can't restore to snapshot {name} because it doesn't exist!");

            _vix.RevertToSnapshot(_vm, snapshot, false);
        }

        public void RemoveSnapshot(string name)
        {
            var snapshot = _vix.GetSnapshots(_vm).FirstOrDefault(s => _vix.GetSnapshotName(s) == name);

            if (snapshot == null)
                throw new SnapshotDoesntExistExceptipon($"Can't remove snapshot {name} because it doesn't exist!");

            _vix.RemoveSnapshot(_vm, snapshot, false);
        }

        public void AddSharedFolder(string name, string path, bool writeaccess)
        {
            _vix.WaitForTools(_vm);
            _vix.LoginToGuest(_vm, Username, Password, false);
            _vix.EnableSharedFolders(_vm);
            _vix.AddSharedFolder(_vm, path, name, writeaccess);
        }

        public string Username { get; set; }
        public string Password { get; set; }

        public IEnumerable<IVMProcess> Processes
        { 
            get
            {
                _vix.WaitForTools(_vm);
                _vix.LoginToGuest(_vm, Username, Password, false);
                return from p in _vix.GetProcesses(_vm) select new VMProcess(p.Name, p.ProcessID);
            }
        }

    public string HypervisorName => "VMwareWorkstation";
        public RemoteProtocol RemoteAccessProtocol { get; private set; }
        public int RemoteAccessPort { get; private set; }
        public string RemoteAccessPassword { get; private set; }

        public void RemoveSharedFolder(string name)
        {
            _vix.RemoveSharedFolder(_vm, name);
        }

        public void ExecuteCommand(string path, string args, bool wait, bool interactive)
        {
            _vix.WaitForTools(_vm);
            _vix.LoginToGuest(_vm, Username, Password, interactive);
            _vix.ExecuteCommand(_vm, path, args, interactive, wait);
        }

        public object ExecutePowershell(string script)
        {
            throw new UnsupportedVMFeature("VMware workstation doesn't support direct execiution of powershell scripts!");
        }

        public void KillProcess(ulong processid)
        {
            _vix.WaitForTools(_vm);
            _vix.LoginToGuest(_vm, Username, Password, false);
            _vix.KillProcess(_vm, processid);
        }

        public void CopyToVM(string hostpath, string guestpath)
        {
            _vix.WaitForTools(_vm);
            _vix.LoginToGuest(_vm, Username, Password, false);
            _vix.CopyFileToGuest(_vm, hostpath, guestpath);
        }

        public void CopyFromVM(string guestpath, string hostpath)
        {
            _vix.WaitForTools(_vm);
            _vix.LoginToGuest(_vm, Username, Password, false);
            _vix.CopyFileToHost(_vm, guestpath, hostpath);
        }

        public bool FileExists(string path)
        {
            _vix.WaitForTools(_vm);
            _vix.LoginToGuest(_vm, Username, Password, false);
            return _vix.FileExists(_vm, path);
        }

        public bool DirectoryExists(string path)
        {
            _vix.WaitForTools(_vm);
            _vix.LoginToGuest(_vm, Username, Password, false);
            return _vix.DirectoryExist(_vm, path);
        }

        public void DeleteFile(string path)
        {
            _vix.WaitForTools(_vm);
            _vix.LoginToGuest(_vm, Username, Password, false);
            _vix.DeleteFileInGuest(_vm, path);
        }

        public void DeleteDirectory(string path)
        {
            _vix.WaitForTools(_vm);
            _vix.LoginToGuest(_vm, Username, Password, false);
            _vix.DeleteDirectoryInGuest(_vm, path);
        }

        public void WaitTillReady()
        {
            while (true)
            {
                if(State == VMState.Off)
                    throw new VMPoweredOffException("VM Powered off while waiting for it to become ready!");

                try
                {
                    _vix.WaitForTools(_vm);
                }
                catch (VixException e) when (e.ErrorCode == Constants.VIX_E_TIMEOUT_WAITING_FOR_TOOLS)
                {
                    continue;
                }

                break;
            }
        }

        public void WaitTillOff()
        {
            var thread = ServiceDiscovery.Instance.Resolve<IThreadWrap>(HypervisorName);
            while (State != VMState.Off)
            {
                thread.Sleep(1000);
            }
        }

        public string ReadEnvironment(string name)
        {
            WaitTillReady();
            _vix.LoginToGuest(_vm, Username, Password, false);
            return _vix.ReadVariable(_vm, name, VixVariable.Environment);
        }

        public string ReadGuestVariable(string name)
        {
            try
            {
                return State == VMState.Off ? 
                    _vmx.ReadVMX($"guestinfo.{name}") : 
                    _vix.ReadVariable(_vm, name, VixVariable.GuestVar);
            }
            catch
            {
                return null;
            }
        }

        public string ReadVMSetting(string name)
        {
            return State == VMState.Off ? _vmx.ReadVMX(name) : _vix.ReadVariable(_vm, name, VixVariable.VMX);
        }

        public void WriteEnvironment(string name, string value)
        {
            WaitTillReady();
            _vix.LoginToGuest(_vm, Username, Password, false);
            _vix.WriteVariable(_vm, name, value, VixVariable.Environment);
        }

        public void WriteGuestVariable(string name, string value)
        {
            if (State == VMState.Off)
            {
                _vmx.WriteVMX($"guestinfo.{name}", value);
                WriteVMX();
            }
            else
                _vix.WriteVariable(_vm, name, value, VixVariable.GuestVar);

            

        }

        public void WriteVMSetting(string name, string value)
        {
            if (State == VMState.Off)
            {
                _vmx.WriteVMX(name, value);
                WriteVMX();
            }
            else
                _vix.WriteVariable(_vm, name, value, VixVariable.VMX);
        }

        public void AddNetworkCard(IVMNetwork network)
        {
            if(State != VMState.Off)
                throw new VMPoweredOnException("Can't add network cards while vm is powered on!");

            _vmx.WriteNetwork(network);
            WriteVMX();
        }

        public IEnumerable<IVMNetwork> GetNetworkCards()
        {
            return _vmx.ReadNetwork();
        }

        public void RemoveNetworkCard(IVMNetwork network)
        {
            if (State != VMState.Off)
                throw new VMPoweredOnException("Can't remove network cards while vm is powered on!");

            _vmx.RemoveNetwork(network);
            WriteVMX();
        }

        public void AddDisk(IVMDisk disk)
        {
            if (State != VMState.Off)
                throw new VMPoweredOnException("Can't add disks while vm is powered on!");

            _vmx.WriteDisk(disk);
            WriteVMX();
        }

        public IEnumerable<IVMDisk> GetDisks()
        {
            return _vmx.ReadDisk();
        }

        public void RemoveDisk(IVMDisk disk)
        {
            if (State != VMState.Off)
                throw new VMPoweredOnException("Can't remove disks while vm is powered on!");

            _vmx.RemoveDisk(disk);
            WriteVMX();
        }

        public void OpenLocalGUI()
        {
            _vix.OpenLocalUI(_vmPath, _hypervisorInfo.Properties["VMwareWorkstationPath"]);
        }

        public void CreateRemoteConnection(int port, string password)
        {
            //Max and Min application ports as per TCP/Windows Standard.
            if(port < 1024 || port > 49151)
                throw new InvalidRemoteConnectionPropertiesException($"Invalid port number {port}. Port expected to be higher than 1023 and lower than 49152.");

            if(string.IsNullOrEmpty(password))
                throw new InvalidRemoteConnectionPropertiesException("You must have a password.");

            RemoteAccessProtocol = RemoteProtocol.VNC;
            RemoteAccessPort = port;
            RemoteAccessPassword = password;

            WriteVMSetting("RemoteDisplay.vnc.enabled", "TRUE");
            WriteVMSetting("RemoteDisplay.vnc.password", password);
            WriteVMSetting("RemoteDisplay.vnc.port", port.ToString());
        }

        public void RemoveRemoteConnection()
        {
            WriteVMSetting("RemoteDisplay.vnc.enabled", "FALSE");
            RemoteAccessProtocol = RemoteProtocol.None;
            RemoteAccessPort = 0;
            RemoteAccessPassword = null;
        }

        private void WriteVMX()
        {
            _file.WriteAllLines(_vmPath, _vmx.ToArray());
        }
    }
}
