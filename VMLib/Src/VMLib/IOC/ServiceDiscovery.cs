using System;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using Ninject;
using Ninject.Modules;

namespace VMLib.IOC
{
    public class ServiceDiscovery : IServiceDiscovery
    {
        private static IServiceDiscovery _serviceDiscovery;

        public static IServiceDiscovery Instance => _serviceDiscovery ?? (_serviceDiscovery = new ServiceDiscovery());

        private readonly StandardKernel _kernel = new StandardKernel();

        public ServiceDiscovery()
        {
            var path = Path.GetDirectoryName(Uri.UnescapeDataString(new UriBuilder(Assembly.GetExecutingAssembly().CodeBase).Path));

            var files = Directory.GetFiles(path, "*.dll");

            foreach(var f in files)
                try
                {
                    Assembly.LoadFile(f);
                }catch (Exception e) { Console.WriteLine($"[E]: {e}"); }

    
                foreach (var a in AppDomain.CurrentDomain.GetAssemblies())
                    try
                    {
                        foreach (var t in a.GetTypes())
                            if (!t.IsAbstract && t.IsSubclassOf(typeof(NinjectModule)) && t != typeof(object))
                            {
                                try
                                {
                                    _kernel.Load((NinjectModule) Activator.CreateInstance(t));
                                }
                                catch
                                {
                                    /* Ignore failed modules */
                                }
                            }
                    } catch { /* Ignore bad dlls */ }
        }

        internal static void UnitTestInjection(IServiceDiscovery fakeobject)
        {
            _serviceDiscovery = fakeobject;
        }

        public IEnumerable<T> ResolveAll<T>()
        {
            return _kernel.GetAll<T>();
        }

        public T Resolve<T>(string name)
        {
            return _kernel.Get<T>(name);
        }

        public T Resolve<T>()
        {
            return _kernel.Get<T>();
        }

        public void AddSingletonType<T, T1>(string name)
        {
            if(!_kernel.CanResolve<T>(name))
                _kernel.Bind(typeof(T)).To(typeof(T1)).InSingletonScope().Named(name);
        }
    }
}
