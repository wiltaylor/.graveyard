using System;
using System.Linq;
using System.Reflection;
using Autofac;
using FlexibleConfigEngine.Core.Gather;
using FlexibleConfigEngine.Core.Resource;

namespace FlexibleConfigEngine.Core.IOC
{
    public class PluginManager : IPluginManager
    {
        private readonly IComponentContext _container;

        public PluginManager(IComponentContext container)
        {
            _container = container;
        }

        private T Resolve<T>(string name)
        {
            var asm = AppDomain.CurrentDomain.GetAssemblies()?
                .Where(a => a.FullName.ToLower().Contains("flexibleconfigengine"));

            var types = asm.SelectMany(a => a.GetTypes()).Where(t => t.IsAssignableTo<T>() && !t.IsInterface);

            var type = types.FirstOrDefault(t => string.Equals(t.GetCustomAttribute<FceItemAttribute>().Name, name, StringComparison.CurrentCultureIgnoreCase));

            if (type == null)
                return default(T);

            var constructor = type.GetConstructors().First();


            return (T)Activator.CreateInstance(type, constructor.GetParameters().Select(para => _container.Resolve(para.ParameterType)).ToArray());
        }

        public IGatherDriver GetGather(string name)
        {

            return Resolve<IGatherDriver>(name);
        }

        public IResourceDriver GetResource(string name)
        {
            return Resolve<IResourceDriver>(name);
        }
    }
}
