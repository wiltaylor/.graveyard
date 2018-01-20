using System;
using System.Collections.Generic;
using System.Linq;
using FlexibleConfigEngine.Core.Exceptions;
using FlexibleConfigEngine.Core.Gather;
using FlexibleConfigEngine.Core.Graph;
using FlexibleConfigEngine.Core.Helper;
using FlexibleConfigEngine.Core.IOC;
using FlexibleConfigEngine.Core.Resource;

namespace FlexibleConfigEngine.Core.Config
{
    public class ConfigManager : IConfigManager
    {
        private readonly IDataStore _dataStore;
        private readonly IGraphManager _graphManager;
        private readonly IPluginManager _pluginManager;
        private readonly IConsole _console;

        public ConfigManager(IDataStore dataStore, IGraphManager graphManager, IPluginManager pluginManager, IConsole console)
        {
            _dataStore = dataStore;
            _graphManager = graphManager;
            _pluginManager = pluginManager;
            _console = console;
        }

        public IEnumerable<ConfigItem> BuildRunList(Dictionary<string, string> settings)
        {
            if(settings != null)
                _dataStore.Write("ConfigSettings", settings);

            var runList = new List<ConfigItem>();
            var notToBeRun = new List<ConfigItem>();
            var pool = new List<ConfigItem>(_graphManager.Config);
            var depthLimit = 50;

            foreach (var item in pool)
            {
                if (item.Dependancy != null && pool.All(p => item.Dependancy != p.Name))
                    throw new MissingDependancyException(item.Dependancy, item.Name);

                if(item.Dependancy == item.Name)
                    throw new SelfReferenceException(item.Name);
            }

            while (pool.Count > 0)
            {
                if(depthLimit <= 0)
                    throw new DeepOrCirtularDependancyException();

                foreach (var configItem in pool)
                {                

                    if (configItem.Criteria.Any(c => !c(_dataStore)))
                    {
                        notToBeRun.Add(configItem);
                        continue;
                    }

                    if (string.IsNullOrEmpty(configItem.Dependancy))
                    {
                        runList.Add(configItem);
                        continue;
                    }

                    if (runList.Any(c => string.Equals(c.Name, configItem.Dependancy, StringComparison.CurrentCultureIgnoreCase)) || 
                        notToBeRun.Any(c => string.Equals(c.Name, configItem.Dependancy, StringComparison.CurrentCultureIgnoreCase)))
                        runList.Add(configItem);
                }

                pool.RemoveAll(i => runList.Contains(i));
                pool.RemoveAll(i => notToBeRun.Contains(i));
                depthLimit--;
            }

            return runList;
        }

        public IEnumerable<ConfigurationResult> ApplyRunList(IEnumerable<ConfigItem> runlist)
        {
            var returnData = new List<ConfigurationResult>();

            foreach (var item in runlist)
            {
                _console.Information("========================================================================");
                _console.Information(" Name: {name}", item.Name);
                _console.Information(" Resource: {resource}", item.Resource);
                _console.Information("========================================================================");
                var currentResult = new ConfigurationResult
                {
                   Name = item.Name,
                   Resource = item.Resource,
                   State = ResourceState.NotConfigured,
                   RanResource = false
                };

                foreach (var dyn in item.Dynamics)
                    dyn(_dataStore, item);

                returnData.Add(currentResult);

                var driver = _pluginManager.GetResource(item.Resource);
                driver.GetData(item);
                var testResult = driver.Test();

                _console.Information("Current Status: {state}", testResult);

                currentResult.State = testResult;

                if (testResult == ResourceState.NeedReboot)
                {
                    currentResult.State = ResourceState.NeedReboot;
                    return returnData;
                }

                if (testResult == ResourceState.NotConfigured)
                {
                    _console.Information("Running Apply to update configuration...");
                    var result = driver.Apply();
                    currentResult.RanResource = true;

                    if (result == ResourceState.NeedReboot)
                    {
                        _console.Information("Reboot Required");
                        currentResult.State = ResourceState.NeedReboot;
                        return returnData;
                    }
                    
                    _console.Information("Finished Apply changes...Rechecking...");
                    var secondTest = driver.Test();

                    if (secondTest != ResourceState.Configured)
                    {
                        _console.Error("Configuration Check failed...exiting...");
                        currentResult.State = ResourceState.NotConfigured;
                        return returnData;
                    }

                    _console.Information("Configuration Verified.");
                }
            }

            return returnData;
        }

        public IEnumerable<ConfigurationResult> Test(IEnumerable<ConfigItem> runlist)
        {
            var returnData = new List<ConfigurationResult>();

            foreach (var item in runlist)
            {
                _console.Information("========================================================================");
                _console.Information(" Name: {name}", item.Name);
                _console.Information(" Resource: {resource}", item.Resource);
                
                var driver = _pluginManager.GetResource(item.Resource);
                driver.GetData(item);
                var currentResult = new ConfigurationResult
                {
                    Name = item.Name,
                    RanResource = false,
                    Resource = item.Resource,
                    State = driver.Test()
                };

                _console.Information(" State: {state}", currentResult.State);
                returnData.Add(currentResult);

                _console.Information("========================================================================");
            }

            return returnData;
        }
    }
}
