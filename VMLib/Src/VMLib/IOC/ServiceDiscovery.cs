using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Reflection;
using Ninject;
using Ninject.Modules;
using Serilog;

namespace VMLib.IOC
{
    public class ServiceDiscovery : IServiceDiscovery
    {
        private static IServiceDiscovery _serviceDiscovery;
        public static IServiceDiscovery Instance => _serviceDiscovery ?? (_serviceDiscovery = new ServiceDiscovery());

        private readonly StandardKernel _kernel = new StandardKernel(new BootStrap());
        private readonly ILogger _log;

        public ServiceDiscovery()
        {
            _log = _kernel.Get<ILogger>();

            var path = Path.GetDirectoryName(Uri.UnescapeDataString(new UriBuilder(Assembly.GetExecutingAssembly().CodeBase).Path));
            var files = Directory.GetFiles(path, "*.dll");

            foreach(var f in files)
                try
                {
                    _log.Information("Loading Assembly {f}", f);
                    Assembly.LoadFile(f);
                }
                catch (Exception e)
                {
                    _log.Warning(e, "Failed to load assembly {f}", f);
                }

    
                foreach (var a in AppDomain.CurrentDomain.GetAssemblies())
                    try
                    {
                        _log.Information("Scanning Assembly {a}", a.FullName);
                        foreach (var t in a.GetTypes())
                            if (!t.IsAbstract && t.IsSubclassOf(typeof(NinjectModule)) && t != typeof(object) && t != typeof(BootStrap))
                            {
                                try
                                {
                                    _log.Information("Loading Ninject Module: {t}", t.FullName);
                                    _kernel.Load((NinjectModule) Activator.CreateInstance(t));
                                }
                                catch (Exception ex)
                                {
                                    _log.Warning(ex, "Failed to load module.");
                                }
                            }
                    }
                    catch (Exception e)
                    {
                        _log.Warning(e, "Unable to load assembly {a}", a);
                    }
        }

        internal static void UnitTestInjection(IServiceDiscovery fakeobject)
        {
            _serviceDiscovery = fakeobject;
        }

        public IEnumerable<T> ResolveAll<T>()
        {
            var result = _kernel.GetAll<T>().ToArray();
            _log.Debug($"Resolving All {nameof(T)}");

            foreach(var t in result)
                _log.Debug("Type: {t}", t.GetType().FullName);

            return result;
        }

        public T Resolve<T>(string name)
        {
            var result = _kernel.Get<T>(name);
            _log.Debug($"Resolving {nameof(T)} as {result.GetType().FullName}");
            return result;
        }

        public T Resolve<T>()
        {
            var result = _kernel.Get<T>();
            _log.Debug($"Resolving {nameof(T)} as {result.GetType().FullName}");
            return result;
        }
    }
}
