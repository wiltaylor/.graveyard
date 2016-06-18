using System.Collections.Generic;
using System.Linq;
using VixCOM;
using VMLib.Exceptions;
using VMLib.VM;
using VMLib.VMware.VIXItems;

namespace VMLib.VMware
{
    public class VMwareVirtualMachine :IVirtualMachine
    {
        private readonly string _vmPath;
        private readonly IVix _vix;
        private readonly IVM2 _vm;

        public VMwareVirtualMachine(string vmPath, IVix vix)
        {
            _vmPath = vmPath;
            _vix = vix;
            _vm = vix.ConnectToVM(vmPath);
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

            if(snapshot == null)
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

        public IEnumerable<IVMProcess> Processes => 
            from p in _vix.GetProcesses(_vm)
            select new VMProcess(p.Name, p.ProcessID);

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

            _vix.WaitForTools(_vm);
        }
    }
}
