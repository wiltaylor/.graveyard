using System;
using System.Collections.Generic;
using System.Linq;
using System.Security.Cryptography;
using System.Text;
using System.Threading.Tasks;
using VMLib.IOC;

namespace VMLib
{
    public abstract class HypervisorInfoBase : IHypervisorInfo
    {
        public virtual string Name => "Base";

        public abstract IHypervisorConnectionInfo CreateConnectionInfo();

        public abstract IHypervisor CreateHypervisor(IHypervisorConnectionInfo info);

        protected void AddTypeToIOC<F, T>()
        {
            ServiceDiscovery.Instance.AddType<F,T>(Name);
        }
    }
}
