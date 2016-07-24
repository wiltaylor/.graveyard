using System;
using System.Collections.Generic;
using System.Linq.Expressions;

namespace VMLib.IOC
{
    public interface IServiceDiscovery
    {
        IEnumerable<T> ResolveAll<T>();
        T Resolve<T>(string name);
        T Resolve<T>();
    }
}
