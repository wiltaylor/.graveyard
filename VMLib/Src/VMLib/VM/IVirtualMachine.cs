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
        void RemoveSharedFolder(string name);
        void ExecuteCommand(string path, string args, bool wait, bool interactive);
        object ExecutePowershell(string script);
        void KillProcess(ulong processid);
        void CopyToVM(string hostpath, string guestpath);
        void CopyFromVM(string guestpath, string hostpath);
        bool FileExists(string path);
        bool DirectoryExists(string path);
        void DeleteFile(string path);
        void DeleteDirectory(string path);
        void WaitTillReady();
    }
}