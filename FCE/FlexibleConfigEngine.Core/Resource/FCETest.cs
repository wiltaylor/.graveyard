using System;
using System.Collections.Generic;
using System.Text;
using FlexibleConfigEngine.Core.Graph;
using FlexibleConfigEngine.Core.IOC;

namespace FlexibleConfigEngine.Core.Resource
{
    [FceItem(Name = "FCETest")]
    public class FceTest : IResourceDriver
    {
        private ConfigItem _data;
        public void GetData(ConfigItem data)
        {
            _data = data;
        }

        public ResourceState Test()
        {
            if (!_data.Properties.ContainsKey("test"))
                return ResourceState.Configured;

            if (_data.Properties["Test"] == "unconfigured")
                return ResourceState.NotConfigured;

            return _data.Properties["Test"] == "reboot" ? ResourceState.NeedReboot : ResourceState.Configured;
        }

        public ResourceState Apply()
        {
            if (!_data.Properties.ContainsKey("Apply"))
                return ResourceState.Configured;

            if (_data.Properties["Apply"] == "unconfigured")
                return ResourceState.NotConfigured;

            return _data.Properties["Apply"] == "reboot" ? ResourceState.NeedReboot : ResourceState.Configured;
        }
    }
}
