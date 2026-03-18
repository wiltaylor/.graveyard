using FCE.Windows.Core.Helpers;
using FlexibleConfigEngine.Core.Graph;
using FlexibleConfigEngine.Core.Helper;
using FlexibleConfigEngine.Core.IOC;
using FlexibleConfigEngine.Core.Resource;

namespace FCE.Windows.Core.Resources
{
    [FceItem(Name="Powershell")]
    public class Powershell : ResourceBase
    {
        public override ResourceState Test(ConfigItem data)
        {
            var script = System.IO.File.ReadAllLines(data.Properties.Get("testscript"));
            var testokstring = data.Properties.Get("testpassstring");

            var text = PowerShellHelper.Run(script);

            return text.Contains(testokstring) ? ResourceState.Configured : ResourceState.NotConfigured;
        }

        public override ResourceState Apply(ConfigItem data)
        {
            var script = System.IO.File.ReadAllLines(data.Properties.Get("applyscript"));
            var needreboot = data.Properties.Get("rebootonapply")?.ToLower() == "true";

            PowerShellHelper.Run(script);

            return needreboot ? ResourceState.NeedReboot : ResourceState.Configured;
        }
    }
}
