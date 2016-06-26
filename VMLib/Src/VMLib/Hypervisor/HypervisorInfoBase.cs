using System;
using VMLib.IOC;

namespace VMLib
{
    public abstract class HypervisorInfoBase : IHypervisorInfo
    {
        public virtual string Name => "Base";

        public abstract IHypervisorConnectionInfo CreateConnectionInfo();

        public IHypervisor CreateHypervisor(IHypervisorConnectionInfo info)
        {
            if(info == null)
                throw new ArgumentNullException(nameof(info));

            var hypervisor =  ServiceDiscovery.Instance.Resolve<IHypervisor>(Name);
            hypervisor.SetConnectionSettings(info);
            return hypervisor;
        }

        protected void AddTypeToIOC<TF, T>()
        {
            ServiceDiscovery.Instance.AddSingletonType<TF,T>(Name);
        }
    }
}
