using System.Collections.Generic;

namespace VMLib
{
    public interface IHypervisorFactory
    {
        IEnumerable<string> GetHypervisorNames();
        IHypervisorConnectionInfo CreateHypervisorConnectionInfo(string hypervisorName);
        IHypervisor CreateHypervisor(string hypervisorName, IHypervisorConnectionInfo connectioninfo);
    }
}
