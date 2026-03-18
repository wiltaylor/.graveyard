using Ninject.Modules;
using VMLib.Disk;
using VMLib.VMware.VIXItems;

namespace VMLib.VMware
{
    public class Bootstrap : NinjectModule
    {
        public override void Load()
        {
            Bind<IHypervisorInfo>().To<VMwareHypervisorInfo>().InSingletonScope();
            Bind<IHypervisor>().To<VMwareHypervisor>().InSingletonScope().Named("VMwareWorkstation");
            Bind<IVMFactory>().To<VMwareVMFactory>().InSingletonScope().Named("VMwareWorkstation");
            Bind<IVix>().To<VIX>().InSingletonScope().Named("VMwareWorkstation");
            Bind<IVMXHelper>().To<VMXHelper>().InSingletonScope().Named("VMwareWorkstation");
            Bind<IDiskBuilder>().To<VMwareDiskBuilder>().InSingletonScope().Named("VMwareWorkstation");

        }
    }
}
