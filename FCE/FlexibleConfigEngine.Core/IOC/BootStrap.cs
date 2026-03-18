using System;
using System.Collections.Generic;
using System.IO;
using System.Text;
using Autofac;

namespace FlexibleConfigEngine.Core.IOC
{
    public class Bootstrap
    {
        private static IContainer _container;

        private static readonly List<Type> WireUpTypes = new List<Type>();

        public static void RegisterForWireUp(Type type)
        {
            WireUpTypes.Add(type);
        }

        public Bootstrap()
        {
            var iocDebug = "IOC Container has been resolved yet.";

            AppDomain.CurrentDomain.UnhandledException += (sender, args) =>
            {
                File.WriteAllText(Environment.ExpandEnvironmentVariables("%temp%\\fcecrash.txt"), args.ExceptionObject.ToString());
                File.WriteAllText(Environment.ExpandEnvironmentVariables("%temp%\\fceiocdump.txt"), iocDebug);

                Console.WriteLine(args.ExceptionObject.ToString());
                Console.WriteLine("FCE has encounted an unexpected error and needs to exit!");

#if DEBUG
                Console.WriteLine("Press any key to exit");
                Console.Read();
#endif

                Environment.Exit(5000);
            };

            var builder = new ContainerBuilder();
            builder.RegisterAssemblyModules(AppDomain.CurrentDomain.GetAssemblies());

            _container = builder.Build();

            var sb = new StringBuilder();

            sb.Append("Loaded Assemblies: ");
            foreach (var a in AppDomain.CurrentDomain.GetAssemblies())
                sb.AppendLine(a.FullName);

            sb.AppendLine("DI Registrations:");
            foreach (var r in _container.ComponentRegistry.Registrations)
            {
                sb.AppendLine("====================================================================================");
                sb.AppendLine(r.ToString());
            }

            iocDebug = sb.ToString();

            //Wire up static objects
            foreach (var type in WireUpTypes)
                WireUpStatic(type);

        }

        public void WireUpStatic(Type type)
        {
            foreach (var prop in type.GetProperties())
            {
                prop.SetValue(null, _container.Resolve(prop.PropertyType));
            }
        }

        public T StarT<T>() => _container.Resolve<T>();

    }
}
