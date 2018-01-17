using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Reflection;
using System.Text;
using CommandLineParser;
using FlexibleConfigEngine.Core.Helper;

namespace FlexibleConfigEngine.CLI
{
    public class Version : CommandHandlerBase
    {
        private readonly IConsole _console;

        public Version(IConsole console)
        {
            _console = console;
        }

        public override void ProcessCommand(string[] args)
        {
            var versioninfo = FileVersionInfo.GetVersionInfo(Assembly.GetEntryAssembly().Location);

            _console.Information("Version: {FileVersion}", versioninfo.FileVersion);
            _console.Information("ProductVersion: {ProductVersion}", versioninfo.ProductVersion);
        }

        public override string PrimaryName => "version";
        public override IEnumerable<string> Names => new[] {"version", "ver", "v"};
        public override string UsageText => "Displays the application version";
    }
}
