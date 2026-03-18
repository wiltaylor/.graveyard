using System;
using Serilog;
using VMLib.IOC;

namespace VMLib
{
    public abstract class HypervisorInfoBase : IHypervisorInfo
    {
        private readonly ILogger _log;

        protected HypervisorInfoBase(ILogger log)
        {
            _log = log;
        }

        public virtual string Name => "Base";

        public abstract IHypervisorConnectionInfo CreateConnectionInfo();

        public IHypervisor CreateHypervisor(IHypervisorConnectionInfo info)
        {
            _log.Debug("Calling CreateHypervisor Info: {@info}", info);
            
            if(info == null)
                throw new ArgumentNullException(nameof(info));

            var hypervisor =  ServiceDiscovery.Instance.Resolve<IHypervisor>(Name);
            hypervisor.SetConnectionSettings(info);
            return hypervisor;
        }
    }
}
