using System;
using System.Collections.Generic;
using System.Text;
using Autofac;
using FlexibleConfigEngine.Core.Config;
using FlexibleConfigEngine.Core.ConfigSession;
using FlexibleConfigEngine.Core.Gather;
using FlexibleConfigEngine.Core.Graph;

namespace FlexibleConfigEngine.Core.IOC
{
    public class ConfigModule : Module
    {
        protected override void Load(ContainerBuilder builder)
        {
            builder.RegisterType<ConfigManager>().AsImplementedInterfaces().SingleInstance();
            builder.RegisterType<Session>().AsImplementedInterfaces();
            builder.RegisterType<DataStore>().AsImplementedInterfaces().SingleInstance();
            builder.RegisterType<GatherManager>().AsImplementedInterfaces().SingleInstance();
            builder.RegisterType<GraphManager>().AsImplementedInterfaces().SingleInstance();

        }
    }
}
