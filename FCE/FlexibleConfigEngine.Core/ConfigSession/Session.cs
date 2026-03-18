using System;
using System.Collections.Generic;
using System.Linq;
using EasyRoslynScript;
using FlexibleConfigEngine.Core.Config;
using FlexibleConfigEngine.Core.Exceptions;
using FlexibleConfigEngine.Core.Gather;
using FlexibleConfigEngine.Core.Graph;
using FlexibleConfigEngine.Core.Helper;
using FlexibleConfigEngine.Core.Resource;
using Newtonsoft.Json;

namespace FlexibleConfigEngine.Core.ConfigSession
{
    public class Session : ISession
    {
        private readonly IScriptRunner _runner;
        private readonly IConsole _console;
        private readonly IGraphManager _graphManager;
        private readonly IGatherManager _gatherManager;
        private readonly IDataStore _dataStore;
        private readonly IFileSystem _fileSystem;
        private readonly IConfigManager _configManager;
        private readonly IEnvironmentHelper _environmentHelper;

        public Session(IScriptRunner runner, IConsole console, IGraphManager graphManager, IGatherManager gatherManager, IDataStore dataStore, IFileSystem fileSystem, IConfigManager configManager, IEnvironmentHelper environmentHelper)
        {
            _runner = runner;
            _console = console;
            _graphManager = graphManager;
            _gatherManager = gatherManager;
            _dataStore = dataStore;
            _fileSystem = fileSystem;
            _configManager = configManager;
            _environmentHelper = environmentHelper;
        }

        public void Apply(string script, string settings, bool testOnly = false)
        {
            if (settings != null && !_fileSystem.FileExist(settings))
            {
                _console.Error("Unable to find settings file {file}", settings);
                return;
            }

            if (!Validate(script))
                return;

            try
            {
                _gatherManager.Run();
            }
            catch (Exception e)
            {
                _console.Error("There was an error while running gathers! Error: {error}", e);
                return;
            }

            try
            {
                var settingData = settings == null
                    ? null
                    : JsonConvert.DeserializeObject<Dictionary<string, string>>(_fileSystem.ReadFile(settings));

                var runlist = _configManager.BuildRunList(settingData);
                var results = testOnly ? _configManager.Test(runlist).ToArray() : _configManager.ApplyRunList(runlist).ToArray();
        
                if (results.Any(r => r.State == ResourceState.NotConfigured))
                    _environmentHelper.SetExitCode(ExitCodes.Error);
                else if (results.Any(r => r.State == ResourceState.NeedReboot))
                    _environmentHelper.SetExitCode(ExitCodes.Reboot);
                else
                    _environmentHelper.SetExitCode(ExitCodes.Ok);
        
                _fileSystem.WriteFile("data.json", _dataStore.GetPersistString());
                _fileSystem.WriteFile("result.json", JsonConvert.SerializeObject(results));
            }
            catch (Exception e)
            {
                _console.Error("There was an error while trying to read {file} Error: {error}", settings, e);
            }

        }

        public void GatherOnly(string script, string output)
        {
            if (!Validate(script))
                return;

            try
            { 
                _gatherManager.Run();
            }
            catch (Exception e)
            {
                _console.Error("There was an error while running gathers! Error: {error}", e);
                return;
            }

            try
            {
                _fileSystem.WriteFile(output, _dataStore.GetPersistString(true));
            }
            catch (Exception e)
            {
                _console.Error("There was an error while writing gather data to disk! Error: {error}", e);
            }
            
        }

        public bool Validate(string script)
        {
            //Loading in data
            if (_fileSystem.FileExist("data.json"))
            {
                var data =
                    JsonConvert.DeserializeObject<Dictionary<string, Dictionary<string, string>>>(
                        _fileSystem.ReadFile("data.json"));

                foreach (var key in data.Keys)
                    _dataStore.Write(key, data[key], true);
            }

            try
            {
                _console.Information("Executing configuration script... This can take awhile if there are a lot of packages to load...");
                _runner.ExecuteFile(script).Wait();
            }
            catch (Exception e)
            {
                _console.Error("There was an error while running script! Error: {error}", e);
                return false;
            }

            try
            {
                _graphManager.Validate();
            }
            catch (GraphValidationException e)
            {
                _console.Error("Validation Error! Error: {details}", e);
                return false;
            }

            return true;
        }
    }
}
