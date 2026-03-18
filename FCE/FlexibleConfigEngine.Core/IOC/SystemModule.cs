using System;
using System.Collections.Generic;
using System.Text;
using Autofac;
using FlexibleConfigEngine.Core.Helper;
using Console = FlexibleConfigEngine.Core.Helper.Console;

namespace FlexibleConfigEngine.Core.IOC
{
    public class SystemModule : Module
    {
        protected override void Load(ContainerBuilder builder)
        {
            builder.RegisterType<Console>().AsImplementedInterfaces();
            builder.RegisterType<EnvironmentHelper>().AsImplementedInterfaces();
            builder.RegisterType<FileSystem>().AsImplementedInterfaces();
            builder.RegisterType<PluginManager>().AsImplementedInterfaces();

        }
    }
}
