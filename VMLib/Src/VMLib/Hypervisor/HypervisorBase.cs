using System.Collections.Generic;
using System.Linq;
using Serilog;
using VMLib.Disk;
using VMLib.Exceptions;
using VMLib.IOC;

namespace VMLib
{
    public abstract class HypervisorBase : IHypervisor
    {
        protected IHypervisorConnectionInfo Settings;
        private readonly ILogger _log;

        protected HypervisorBase(ILogger log)
        {
            _log = log;
        }

        public void SetConnectionSettings(IHypervisorConnectionInfo settings)
        {
            if (Settings != null)
            {
                _log.Error("Trying to set Connection twice...");
                throw new HypervisorAlreadySetupException(
                    "You can't call SetConnectionSettings. It is only used during setup.");
            }

            Settings = settings;
        }

        public abstract string Name { get; }

        protected virtual IVMFactory CreateFactory()
        {
            _log.Debug("Calling Create Factory. Name: {Name}", Name);
            var factory = ServiceDiscovery.Instance.Resolve<IVMFactory>(Name);
            factory.SetConnectionInfo(Settings);
            return factory;
        }

        public IVirtualMachine CreateNewVM(IVMCreationInfo info)
        {
            _log.Debug("Calling Create New VM. Info: {@info}", info);
            var factory = CreateFactory();
            return factory.Create(info);
        }

        public IVirtualMachine OpenVM(string path)
        {
            _log.Debug("Calling OpenVM. Path: {path}", path);
            var factory = CreateFactory();
            return factory.Open(path);
        }

        public IEnumerable<IVirtualMachine> GetAllRunningVM()
        {
            
            var factory = CreateFactory();
            var vms = factory.GetAllRunning().ToArray();
            _log.Debug("Calling GetAllRunningVM: {vms}", vms);
            return vms;
        }

        public IDiskBuilder GetDiskBuilder()
        {
            _log.Debug("Calling GetDiskBuilder");
            return ServiceDiscovery.Instance.Resolve<IDiskBuilder>(Name);
        }

        public abstract string[] GetAllVMsInFolder(string path);
        public abstract string Extension { get; }
    }
}
