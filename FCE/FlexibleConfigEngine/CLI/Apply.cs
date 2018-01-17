using System;
using System.Collections.Generic;
using System.Linq;
using CommandLineParser;
using FlexibleConfigEngine.Core.ConfigSession;
using FlexibleConfigEngine.Core.Helper;

namespace FlexibleConfigEngine.CLI
{
    public class Apply : CommandHandlerBase
    {
        private readonly IFileSystem _fileSystem;
        private readonly IConsole _console;
        private readonly ISession _session;

        public Apply(IFileSystem fileSystem, IConsole console, ISession session)
        {
            _fileSystem = fileSystem;
            _console = console;
            _session = session;
        }

        public override void ProcessCommand(string[] args)
        {
            var script = GetSwitch("script")?.FirstOrDefault() ?? "config.csx";
            var settings = GetSwitch("setting")?.FirstOrDefault();

            if (!_fileSystem.FileExist(script))
            {
                _console.Error("The configuration script {script} is missing! Please make sure you are in the right directory and try again.", script);
                return;
            }

            if (!string.IsNullOrEmpty(settings) && !_fileSystem.FileExist(settings))
            {
                _console.Error("Can't find settings file {file}.", settings);
                return;
            }

            try
            {
                _session.Apply(script, settings);
            }
            catch (Exception e)
            {
                _console.Error("There was a problem running apply! Error Details: {error}", e);
            }

        }

        public override string PrimaryName => "apply";
        public override IEnumerable<string> Names => new[] {"apply", "a", "commit", "run"};
        public override string UsageText => "Applies configuration script";

        public override IEnumerable<SwitchInfo> Switches => new []
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
                Names = new [] { "setting", "settings"},
                ShortNames = new [] {"c"},
                UsageText = "Specify a settings file to pass in with the script."
            }
        };
    }
}
