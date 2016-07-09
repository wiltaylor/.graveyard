using System.Collections;
using System.Collections.Generic;
using VMLib.VM;

namespace VMLib
{
    public enum VMState
    {
        Off,
        Ready,
        Pending
    }

    public enum RemoteProtocol
    {
        None,
        RPD,
        VNC
    }
    public interface IVirtualMachine
    {
        VMState State { get; }
        IEnumerable<string> Snapshots { get;}
        void Start();
        void Stop(bool force);
        void Restart(bool force);
        void CreateSnapshot(string name, string description);
        void RestoreSnapshot(string name);
        void RemoveSnapshot(string name);
        void AddSharedFolder(string name, string path, bool writeaccess);
        string Username { get; set; }
        string Password { get; set; }
        IEnumerable<IVMProcess> Processes { get; }
        string HypervisorName { get; }
        RemoteProtocol RemoteAccessProtocol { get; }
        int RemoteAccessPort { get; }
        string RemoteAccessPassword { get; }
        void RemoveSharedFolder(string name);
        void ExecuteCommand(string path, string args, bool wait, bool interactive);
        object ExecutePowershell(string script, Hashtable arguments);
        void KillProcess(ulong processid);
        void CopyToVM(string hostpath, string guestpath);
        void CopyFromVM(string guestpath, string hostpath);
        bool FileExists(string path);
        bool DirectoryExists(string path);
        void DeleteFile(string path);
        void DeleteDirectory(string path);
        void WaitTillReady();
        void WaitTillOff();
        string ReadEnvironment(string name);
        string ReadGuestVariable(string name);
        string ReadVMSetting(string name);
        void WriteEnvironment(string name, string value);
        void WriteGuestVariable(string name, string value);
        void WriteVMSetting(string name, string value);
        void AddNetworkCard(IVMNetwork network);
        IEnumerable<IVMNetwork> GetNetworkCards();
        void RemoveNetworkCard(IVMNetwork network);
        void AddDisk(IVMDisk disk);
        IEnumerable<IVMDisk> GetDisks();
        void RemoveDisk(IVMDisk disk);
        void OpenLocalGUI();
        void CreateRemoteConnection(int port, string password);
        void RemoveRemoteConnection();

        string VMPath { get; }
        string VMDirectory { get; }
        string Name { get; set; }
        void Clone(string path, string snapshot, bool linked);
    }
}