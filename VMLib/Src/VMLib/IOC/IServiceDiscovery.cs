using System;
using System.Collections.Generic;
using System.Linq.Expressions;

namespace VMLib.IOC
{
    public interface IServiceDiscovery
    {
        IEnumerable<T> ResolveAll<T>();
        void AddType<T, T1>(string name);
        void AddType<T, T1>();
        T Resolve<T>(string name);
        T Resolve<T>();
    }
}
