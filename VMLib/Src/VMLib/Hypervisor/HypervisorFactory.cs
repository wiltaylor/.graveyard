using System.Collections.Generic;
using System.Linq;
using VMLib.Exceptions;
using VMLib.IOC;

namespace VMLib
{
    public class HypervisorFactory : IHypervisorFactory
    {
        public IEnumerable<string> GetHypervisorNames()
        {
            return ServiceDiscovery.Instance
                .ResolveAll<IHypervisorInfo>()
                .Select(i => i.Name);
        }

        public IHypervisorConnectionInfo CreateHypervisorConnectionInfo(string hypervisorName)
        {
            var hyperviosor = ServiceDiscovery.Instance
                .ResolveAll<IHypervisorInfo>()
                .FirstOrDefault(h => h.Name == hypervisorName);

            if(hyperviosor == null)
                throw new UnknownHypervisorException($"Hypervisor {hypervisorName} doesn't exist! Please call GetHypervisorNames to get available hypervisors!");

            return hyperviosor.CreateConnectionInfo();
        }

        public IHypervisor CreateHypervisor(string hypervisorName, IHypervisorConnectionInfo info)
        {
            var hyperviosor = ServiceDiscovery.Instance
                .ResolveAll<IHypervisorInfo>()
                .FirstOrDefault(h => h.Name == hypervisorName);

            if (hyperviosor == null)
                throw new UnknownHypervisorException($"Hypervisor {hypervisorName} doesn't exist! Please call GetHypervisorNames to get available hypervisors!");

            return hyperviosor.CreateHypervisor(info);
        }
    }
}
