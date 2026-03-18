using Ninject.Modules;

namespace VMLib.HyperV
{
    public class Bootstrap : NinjectModule
    {
        public override void Load()
        {
            Bind<IHypervisor>().To<HyperVHypervisor>().InSingletonScope();
        }
    }
}
