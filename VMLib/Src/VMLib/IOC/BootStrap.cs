using SystemWrapper.IO;
using SystemWrapper.Microsoft.Win32;
using SystemWrapper.Threading;
using Ninject.Modules;
using VMLib.Utility;

namespace VMLib.IOC
{
    public class BootStrap : NinjectModule
    {
        public override void Load()
        {
            Bind<IRegistryWrap>().To<RegistryWrap>();
            Bind<IRegistryKeyWrap>().To<RegistryKeyWrap>();
            Bind<IFileWrap>().To<FileWrap>();
            Bind<IThreadWrap>().To<ThreadWrap>();
            Bind<IEnvironmentHelper>().To<EnvironmentHelper>();
        }
    }
}
