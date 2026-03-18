using System;
using System.Collections;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using VMLib.VM;

namespace VMLib.Shims
{
    public abstract class ShimVirtualMachineBase : IVirtualMachine
    {
        protected readonly IVirtualMachine Basevm;

        protected ShimVirtualMachineBase(IVirtualMachine basevm)
        {
            Basevm = basevm;
        }

        public virtual VMState State => Basevm.State;

        public virtual IEnumerable<string> Snapshots => Basevm.Snapshots;

        public virtual void Start()
        {
            Basevm.Start();
        }

        public virtual void Stop(bool force)
        {
            Basevm.Stop(force);
        }

        public virtual void Restart(bool force)
        {
            Basevm.Restart(force);
        }

        public virtual void CreateSnapshot(string name, string description)
        {
            Basevm.CreateSnapshot(name, description);
        }

        public virtual void RestoreSnapshot(string name)
        {
            Basevm.RestoreSnapshot(name);
        }

        public virtual void RemoveSnapshot(string name)
        {
            Basevm.RemoveSnapshot(name);
        }

        public virtual void AddSharedFolder(string name, string path, bool writeaccess)
        {
            Basevm.AddSharedFolder(name, path, writeaccess);
        }

        public virtual string Username
        {
            get { return Basevm.Username; }
            set { Basevm.Username = value; }
        }

        public virtual string Password
        {
            get { return Basevm.Password; }
            set { Basevm.Password = value; }
        }

        public virtual IEnumerable<IVMProcess> Processes => Basevm.Processes;
        public virtual string HypervisorName => Basevm.HypervisorName;
        public virtual RemoteProtocol RemoteAccessProtocol => Basevm.RemoteAccessProtocol;
        public virtual int RemoteAccessPort => Basevm.RemoteAccessPort;
        public virtual string RemoteAccessPassword => Basevm.RemoteAccessPassword;
        public virtual void RemoveSharedFolder(string name)
        {
            Basevm.RemoveSharedFolder(name);
        }

        public virtual void ExecuteCommand(string path, string args, bool wait, bool interactive)
        {
            Basevm.ExecuteCommand(path, args, wait, interactive);
        }

        public virtual PowershellResults ExecutePowershell(string script, Hashtable arguments)
        {
            return Basevm.ExecutePowershell(script, arguments);
        }

        public virtual void KillProcess(ulong processid)
        {
            Basevm.KillProcess(processid);
        }

        public virtual void CopyToVM(string hostpath, string guestpath)
        {
            Basevm.CopyToVM(hostpath, guestpath);
        }

        public virtual void CopyFromVM(string guestpath, string hostpath)
        {
            Basevm.CopyFromVM(guestpath, hostpath);
        }

        public virtual bool FileExists(string path)
        {
            return Basevm.FileExists(path);
        }

        public virtual bool DirectoryExists(string path)
        {
            return Basevm.DirectoryExists(path);
        }

        public virtual void DeleteFile(string path)
        {
            Basevm.DeleteFile(path);
        }

        public virtual void DeleteDirectory(string path)
        {
            Basevm.DeleteDirectory(path);
        }

        public virtual void WaitTillReady()
        {
            Basevm.WaitTillReady();
        }

        public virtual void WaitTillOff()
        {
            Basevm.WaitTillOff();
        }

        public virtual string ReadEnvironment(string name)
        {
            return Basevm.ReadEnvironment(name);
        }

        public virtual string ReadGuestVariable(string name)
        {
            return Basevm.ReadGuestVariable(name);
        }

        public virtual string ReadVMSetting(string name)
        {
            return Basevm.ReadVMSetting(name);
        }

        public virtual void WriteEnvironment(string name, string value)
        {
            Basevm.WriteEnvironment(name, value);
        }

        public virtual void WriteGuestVariable(string name, string value)
        {
            Basevm.WriteGuestVariable(name, value);
        }

        public virtual void WriteVMSetting(string name, string value)
        {
            Basevm.WriteVMSetting(name, value);
        }

        public virtual void AddNetworkCard(IVMNetwork network)
        {
            Basevm.AddNetworkCard(network);
        }

        public virtual IEnumerable<IVMNetwork> GetNetworkCards()
        {
            return Basevm.GetNetworkCards();
        }

        public virtual void RemoveNetworkCard(IVMNetwork network)
        {
            Basevm.RemoveNetworkCard(network);
        }

        public virtual void AddDisk(IVMDisk disk)
        {
            Basevm.AddDisk(disk);
        }

        public virtual IEnumerable<IVMDisk> GetDisks()
        {
            return Basevm.GetDisks();
        }

        public virtual void RemoveDisk(IVMDisk disk)
        {
            Basevm.RemoveDisk(disk);
        }

        public virtual void OpenLocalGUI()
        {
            Basevm.OpenLocalGUI();
        }

        public virtual void CreateRemoteConnection(int port, string password)
        {
            Basevm.CreateRemoteConnection(port, password);
        }

        public virtual void RemoveRemoteConnection()
        {
            Basevm.RemoveRemoteConnection();
        }

        public virtual string VMPath => Basevm.VMPath;
        public virtual string VMDirectory => Basevm.VMDirectory;
        public virtual string Name {
            get { return Basevm.Name; }
            set { Basevm.Name = value; } }

        public virtual int Memory
        {
            get { return Basevm.Memory; }
            set { Basevm.Memory = value; }
        }

        public virtual int CPU
        {
            get { return Basevm.CPU; }
            set { Basevm.CPU = value; }
        }

        public virtual int CPUCores
        {
            get { return Basevm.CPUCores; }
            set { Basevm.CPUCores = value; }
        }

        public virtual void Clone(string path, string snapshot, bool linked)
        {
            Basevm.Clone(path, snapshot, linked);
        }

        public virtual void DeleteVM()
        {
            Basevm.DeleteVM();
        }

        public virtual void RenameFile(string path, string newPath)
        {
            Basevm.RenameFile(path, newPath);
        }

        public virtual void RenameDirectory(string path, string newPath)
        {
            Basevm.RenameDirectory(path, newPath);
        }
    }
}
