using Autofac;
using CommandLineParser;
using FlexibleConfigEngine.CLI;

namespace FlexibleConfigEngine.IOC
{
    public class CommandLineModule : Module
    {
        protected override void Load(ContainerBuilder builder)
        {
            builder.RegisterType<Apply>().AsImplementedInterfaces();
            builder.RegisterType<Gather>().AsImplementedInterfaces();
            builder.RegisterType<Test>().AsImplementedInterfaces();
            builder.RegisterType<Validate>().AsImplementedInterfaces();
            builder.RegisterType<Version>().AsImplementedInterfaces();

            builder.RegisterType<CommandParser>().AsImplementedInterfaces();

        }
    }
}
