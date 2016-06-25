using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Reflection;
using VMLib.Exceptions;
using VMLib.IOC;

namespace VMLib
{
    public class HypervisorFactory : IHypervisorFactory
    {
        public HypervisorFactory()
        {
            var dllfolder = System.IO.Path.GetDirectoryName(Assembly.GetExecutingAssembly().Location);


            if (string.IsNullOrEmpty(dllfolder))
                throw new ApplicationException("Unable to find directory dll is running from. Not sure why this happened.");

            foreach (var file in Directory.GetFiles(dllfolder, "vmlib.*.dll"))
            {
                try
                {
                    Assembly.LoadFile(file);
                }
                catch { /* Skip libs that can't be loaded */}
            }

            foreach (var asm in AppDomain.CurrentDomain.GetAssemblies())
            {
                foreach (var typ in asm.GetTypes())
                {
                    if (typ.IsAssignableFrom(typeof(IHypervisorInfo)) && typ != typeof(IHypervisorInfo))
                    {
                        ServiceDiscovery.Instance.AddType(typeof(IHypervisorInfo), typ);
                    }
                }
            }
            
        }

        internal HypervisorFactory(bool unitTest)
        {
            //Don't load dlls during unit test.
        }

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
