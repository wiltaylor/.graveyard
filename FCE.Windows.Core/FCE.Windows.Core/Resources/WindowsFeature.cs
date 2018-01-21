using FCE.Windows.Core.Helpers;
using FlexibleConfigEngine.Core.Graph;
using FlexibleConfigEngine.Core.Helper;
using FlexibleConfigEngine.Core.IOC;
using FlexibleConfigEngine.Core.Resource;

namespace FCE.Windows.Core.Resources
{
    [FceItem(Name = "WindowsFeature")]
    public class WindowsFeature : ResourceBase
    {
        public override ResourceState Test(ConfigItem data)
        {
            var name = data.Properties.Get("name"); 
            var ensure = data.Properties.Get("ensure");

            var installed = PowerShellHelper.Run($"(Get-WindowsOptionalFeature -online -FeatureName {name}).State")
                .Contains("Enabled");

            if (ensure.ToLower() == "present" && installed)
                return ResourceState.Configured;
            if (ensure.ToLower() == "absent" && !installed)
                return ResourceState.Configured;
            return ResourceState.NotConfigured;
        }

        public override ResourceState Apply(ConfigItem data)
        {
            var name = data.Properties.Get("name");
            var ensure = data.Properties.Get("ensure");

            if (ensure.ToLower() == "present")
            {
                if (PowerShellHelper
                    .Run(
                        $"(Enable-WindowsOptionalFeature -FeatureName {name} -Online -NoRestart).RestartNeeded")
                    .Contains("True"))
                    return ResourceState.NeedReboot;
            }

            if (ensure.ToLower() == "absent")
            {
                if (PowerShellHelper
                    .Run(
                        $"(Disable-WindowsOptionalFeature -FeatureName {name} -Online -NoRestart).RestartNeeded")
                    .Contains("True"))
                    return ResourceState.NeedReboot;
            }

            return ResourceState.Configured;
        }
    }
}
