using Autofac;
using EasyNuGet;
using EasyRoslynScript;
using EasyRoslynScript.NuGet;
using FlexibleConfigEngine.Core.Script;

namespace FlexibleConfigEngine.Core.IOC
{
    public class ScriptModule : Module
    {
        protected override void Load(ContainerBuilder builder)
        {
            builder.RegisterType<ScriptRunner>().AsImplementedInterfaces();
            builder.RegisterType<NugetPreProcessor>().AsImplementedInterfaces();
            builder.RegisterType<ServiceLocator>().AsImplementedInterfaces();
            builder.RegisterType<PackageDownloader>().AsImplementedInterfaces();
            builder.RegisterType<PackageSearcher>().AsImplementedInterfaces();
            builder.RegisterType<PackageUploader>().AsImplementedInterfaces();
            builder.RegisterType<ExtensionMethodHandler>().AsImplementedInterfaces();
            builder.RegisterType<NuGetScriptSettings>().AsImplementedInterfaces();
            builder.RegisterType<ScriptBootStrap>().AsImplementedInterfaces();

            Bootstrap.RegisterForWireUp(typeof(FluentScriptMethods));
        }
    }
}
