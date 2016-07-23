using System.Collections.Generic;
using VMLib.Disk;

namespace VMLib
{
    public interface IHypervisor
    {
        void SetConnectionSettings(IHypervisorConnectionInfo settings);
        string Name { get; }
        IVirtualMachine CreateNewVM(IVMCreationInfo info);
        IVirtualMachine OpenVM(string path);
        IEnumerable<IVirtualMachine> GetAllRunningVM();
        IDiskBuilder GetDiskBuilder();
        string[] GetAllVMsInFolder(string path);
    }
}
