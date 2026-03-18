using SystemWrapper.IO;
using SystemWrapper.Microsoft.Win32;
using SystemWrapper.Threading;
using Ninject.Modules;
using Serilog;
using VMLib.Hypervisor;
using VMLib.Utility;

namespace VMLib.IOC
{
    public class BootStrap : NinjectModule
    {
        public override void Load()
        {
            Bind<IHypervisorFactory>().To<HypervisorFactory>().InSingletonScope();
            Bind<IRegistryWrap>().To<RegistryWrap>();
            Bind<IRegistryKeyWrap>().To<RegistryKeyWrap>();
            Bind<IFileWrap>().To<FileWrap>();
            Bind<IThreadWrap>().To<ThreadWrap>();
            Bind<IEnvironmentHelper>().To<EnvironmentHelper>();

            Bind<ILogger>().ToConstant(new LoggerConfiguration()
                .WriteTo.LiterateConsole()
                .WriteTo.RollingFile("vmlib-{Date}.log")
                .CreateLogger()).InSingletonScope();

        }
    }
}
