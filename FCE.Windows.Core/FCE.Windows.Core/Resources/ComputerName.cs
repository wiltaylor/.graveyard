using System;
using FCE.Windows.Core.Helpers;
using FlexibleConfigEngine.Core.Graph;
using FlexibleConfigEngine.Core.Helper;
using FlexibleConfigEngine.Core.IOC;
using FlexibleConfigEngine.Core.Resource;

namespace FCE.Windows.Core.Resources
{
    [FceItem(Name = "ComputerName")]
    public class ComputerName : ResourceBase
    {
        public override ResourceState Test(ConfigItem data)
        {
            var computerName = data.Properties.Get("name").ToLower();

            return computerName == Environment.MachineName.ToLower()
                ? ResourceState.Configured
                : ResourceState.NotConfigured;
        }

        public override ResourceState Apply(ConfigItem data)
        {
            var computerName = data.Properties.Get("name");

            PowerShellHelper.Run($"Rename-Computer -NewName {computerName}");

            return ResourceState.NeedReboot;
        }
    }
}
