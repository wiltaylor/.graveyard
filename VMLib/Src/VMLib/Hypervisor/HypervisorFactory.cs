using System.Collections.Generic;
using System.Linq;
using Serilog;
using VMLib.Exceptions;
using VMLib.IOC;

namespace VMLib.Hypervisor
{
    public class HypervisorFactory : IHypervisorFactory
    {

        private readonly ILogger _log;

        public HypervisorFactory(ILogger log)
        {
            _log = log;
        }

        public IEnumerable<string> GetHypervisorNames()
        {
            var hyper = ServiceDiscovery.Instance
                .ResolveAll<IHypervisorInfo>()
                .Select(i => i.Name)
                .ToArray();

            _log.Information("GetHypervisorNames: {@hyper} ", hyper);

            return hyper;
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

        public static IHypervisorFactory GetInstance()
        {
            //This is a work around for powershell not being able to retrive object with generic interface.
            return ServiceDiscovery.Instance.Resolve<IHypervisorFactory>();
        }
    }
}
