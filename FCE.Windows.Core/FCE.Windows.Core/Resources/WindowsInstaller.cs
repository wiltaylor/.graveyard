using System.IO;
using FCE.Windows.Core.Helpers;
using FlexibleConfigEngine.Core.Graph;
using FlexibleConfigEngine.Core.Helper;
using FlexibleConfigEngine.Core.IOC;
using FlexibleConfigEngine.Core.Resource;

namespace FCE.Windows.Core.Resources
{
    [FceItem(Name="WindowsInstaller")]
    public class WindowsInstaller : ResourceBase
    {
        public override ResourceState Test(ConfigItem data)
        {
            var productcode = data.Properties.Get("productcode");
            var ensure = data.Properties.Get("ensure");
            var found = PowerShellHelper
                .Run($"Test-Path 'HKLM:\\SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{productcode}'")
                .Contains("True") || 
                PowerShellHelper.Run($"Test-Path 'HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{productcode}'")
                .Contains("True");

            switch (ensure)
            {
                case "present" when found:
                    return ResourceState.Configured;
                case "absent" when !found:
                    return ResourceState.Configured;
            }

            return ResourceState.NotConfigured;
        }

        public override ResourceState Apply(ConfigItem data)
        {
            var path = data.Properties.Get("path");
            var transforms = data.Properties.Get("transforms");
            var args = data.Properties.Get("extraarguments");
            var productcode = data.Properties.Get("productcode");
            var ensure = data.Properties.Get("ensure");

            var workingdir = Path.GetDirectoryName(path);

            if (ensure == "present")
            {
                var ops = " ";

                if (transforms != default(string))
                    args += $"TRANSFORMS={transforms} ";

                if (args != default(string))
                    ops += args;

                if (PowerShellHelper.Run(new[]
                {
                    $"Set-Location '{workingdir}'",
                    $"&msiexec -i \"{path}\" -qn REBOOT=ReallySupress {ops}",
                    "if($LASTEXITCODE -eq 3010) { Write-Host '!!!REBOOT!!!'}"

                }).Contains("!!!REBOOT!!!"))
                    return ResourceState.NeedReboot;
            }

            if (ensure == "absent")
            {
                PowerShellHelper.Run($"&msiexec -x '{productcode}' -qn REBOOT=ReallySupress");
            }

            return ResourceState.Configured;

        }
    }
}
