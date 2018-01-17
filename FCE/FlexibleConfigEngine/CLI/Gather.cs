using System;
using System.Collections.Generic;
using System.Linq;
using CommandLineParser;
using FlexibleConfigEngine.Core.ConfigSession;
using FlexibleConfigEngine.Core.Helper;

namespace FlexibleConfigEngine.CLI
{
    public class Gather : CommandHandlerBase
    {
        private readonly IFileSystem _fileSystem;
        private readonly IConsole _console;
        private readonly ISession _session;

        public Gather(IFileSystem fileSystem, IConsole console, ISession session)
        {
            _fileSystem = fileSystem;
            _console = console;
            _session = session;
        }

        public override void ProcessCommand(string[] args)
        {
            var script = GetSwitch("script")?.FirstOrDefault() ?? "config.csx";
            var output = GetSwitch("output")?.FirstOrDefault() ?? "gather.json";

            if (!_fileSystem.FileExist(script))
            {
                _console.Error("The configuration script {script} is missing! Please make sure you are in the right directory and try again.", script);
                return;
            }

            try
            { 
                _session.GatherOnly(script, output);
            }
            catch (Exception e)
            {
                _console.Error("There was a problem running apply! Error Details: {error}", e);
            }
        }

        public override string PrimaryName => "gather";
        public override IEnumerable<string> Names => new[] {"gather", "g"};
        public override string UsageText => "Only runs gathers in config script and stores results in json file.";

        public override IEnumerable<SwitchInfo> Switches => new[]
        {
            new SwitchInfo
            {
                ArgumentCount = 1,
                Names = new [] { "script"},
                ShortNames = new [] {"s"},
                UsageText = "Specify another csx file to process rather than config.csx"
            },
            new SwitchInfo
            {
                ArgumentCount = 1,
                Names = new []{"output"},
                ShortNames = new []{"o"},
                UsageText = "Specify output file for gather data."
            }
        };
    }
}
