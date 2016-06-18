using System.Collections.Generic;

namespace VMLib.IOC
{
    public interface IServiceDiscovery
    {
        IEnumerable<T> ResolveAll<T>();
        void AddType<T, T1>(string name);

    }
}
