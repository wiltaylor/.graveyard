using System.Collections.Generic;

namespace VMLib
{
    public interface IVMFactory
    {
        IVirtualMachine Create(IVMCreationInfo info);
        IVirtualMachine Open(string path);
        IEnumerable<IVirtualMachine> GetAllRunning();
        void SetConnectionInfo(IHypervisorConnectionInfo info);
    }
}
