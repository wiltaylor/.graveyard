using Ninject.Modules;

namespace VMLib.VirtualBox
{
    public class Bootstap : NinjectModule
    {
        public override void Load()
        {
            Bind<IHypervisor>().To<VirtualBoxHypervisor>().InSingletonScope();
        }
    }
}
