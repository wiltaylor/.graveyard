using SystemWrapper.IO;
using SystemWrapper.Microsoft.Win32;
using VMLib.IOC;

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
        }
    }
}
