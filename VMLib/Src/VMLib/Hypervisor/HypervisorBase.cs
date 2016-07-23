using System.Collections.Generic;
using VMLib.Disk;
using VMLib.Exceptions;
using VMLib.IOC;

namespace VMLib
{
    public abstract class HypervisorBase : IHypervisor
    {
        protected IHypervisorConnectionInfo Settings;

        public void SetConnectionSettings(IHypervisorConnectionInfo settings)
        {
            if (Settings != null)
                throw new HypervisorAlreadySetupException(
                    "You can't call SetConnectionSettings. It is only used during setup.");

            Settings = settings;
        }

        public abstract string Name { get; }

        protected virtual IVMFactory CreateFactory()
        {
            var factory = ServiceDiscovery.Instance.Resolve<IVMFactory>(Name);
            factory.SetConnectionInfo(Settings);
            return factory;
        }

        public IVirtualMachine CreateNewVM(IVMCreationInfo info)
        {
            var factory = CreateFactory();
            return factory.Create(info);
        }

        public IVirtualMachine OpenVM(string path)
        {
            var factory = CreateFactory();
            return factory.Open(path);
        }

        public IEnumerable<IVirtualMachine> GetAllRunningVM()
        {
            var factory = CreateFactory();
            return factory.GetAllRunning();
        }

        public IDiskBuilder GetDiskBuilder()
        {
            return ServiceDiscovery.Instance.Resolve<IDiskBuilder>(Name);
        }

        public abstract string[] GetAllVMsInFolder(string path);

    }
}
