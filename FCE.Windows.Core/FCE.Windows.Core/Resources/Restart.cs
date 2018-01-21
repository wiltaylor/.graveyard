using System.Collections.Generic;
using System.Diagnostics;
using FlexibleConfigEngine.Core.Gather;
using FlexibleConfigEngine.Core.Graph;
using FlexibleConfigEngine.Core.IOC;
using FlexibleConfigEngine.Core.Resource;
using FlexibleConfigEngine.Core.Helper;

namespace FCE.Windows.Core
{
    [FceItem(Name="Restart")]
    public class Restart : ResourceBase
    {
        private readonly IDataStore _datastore;

        public Restart(IDataStore datastore)
        {
            _datastore = datastore;
        }

        public override ResourceState Test(ConfigItem data)
        {
            var restartblock = _datastore.Read("RebootBlock");

            if (restartblock == null)
                return ResourceState.NotConfigured;

            return restartblock.ContainsKey(data.Name) ? ResourceState.Configured : ResourceState.NotConfigured;
        }

        public override ResourceState Apply(ConfigItem data)
        {
            if (data.Properties.ContainsKey("ForceReboot"))
            {
                Process.Start("shutdown.exe", "/r /t 30");
            }

            var restartblock = (Dictionary<string, string>)_datastore.Read("RebootBlock") ?? new Dictionary<string, string>();

            if(!restartblock.ContainsKey(data.Name))
                restartblock.Add(data.Name, "True");
            
           _datastore.Write("RebootBlock", restartblock, true);

            return ResourceState.NeedReboot;
        }
    }
}
