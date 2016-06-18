using System.Collections.Generic;

namespace VMLib
{
    public interface IHypervisor
    {
        void SetConnectionSettings(IHypervisorConnectionInfo settings);
        IVMCreationInfo CreateNewVMInfo();
        string Name { get; }
        IVirtualMachine CreateNewVM(IVMCreationInfo info);
        IVirtualMachine OpenVM(string path);
        IEnumerable<IVirtualMachine> GetAllRunningVM();
    }
}
