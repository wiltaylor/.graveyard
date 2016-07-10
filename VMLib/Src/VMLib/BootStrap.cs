using SystemWrapper.IO;
using SystemWrapper.Microsoft.Win32;
using SystemWrapper.Threading;
using VMLib.IOC;
using VMLib.Utility;

namespace VMLib
{
    public class BootStrap
    {
        public void Load()
        {
            var container = ServiceDiscovery.Instance;

            container.AddType<IRegistryWrap, RegistryWrap>();
            container.AddType<IRegistryKeyWrap, RegistryKeyWrap>();
            container.AddType<IFileWrap, FileWrap>();
            container.AddType<IThreadWrap, ThreadWrap>();
            container.AddType<IEnvironmentHelper, EnvironmentHelper>();
        }
    }
}
