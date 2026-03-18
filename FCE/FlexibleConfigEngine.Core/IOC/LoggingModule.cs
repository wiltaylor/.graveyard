using System;
using Autofac;
using FlexibleConfigEngine.Core.Helper;
using Serilog;
using Serilog.Filters;

namespace FlexibleConfigEngine.Core.IOC
{
    public class LoggingModule : Module
    {
        protected override void Load(ContainerBuilder builder)
        {
            var logfolder = $"{Environment.CurrentDirectory}";

            builder.RegisterInstance<ILogger>(new LoggerConfiguration()
                    .WriteTo.RollingFile(logfolder + "\\fce-{Date}.log")
                    .WriteTo.Logger(conlog => conlog
                        .Filter.ByIncludingOnly(Matching.WithProperty<bool>("Console", p => p))
                        .WriteTo.LiterateConsole(outputTemplate: "{Message}\n")
                    ).CreateLogger())
                .SingleInstance();

            builder.RegisterType<Helper.Console>().As<IConsole>();
        }
    }
}
