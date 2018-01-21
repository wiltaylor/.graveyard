using System.Collections.Generic;
using FCE.Windows.Core.Helpers;
using FlexibleConfigEngine.Core.Exceptions;
using FlexibleConfigEngine.Core.Graph;
using FlexibleConfigEngine.Core.Helper;
using FlexibleConfigEngine.Core.IOC;
using FlexibleConfigEngine.Core.Resource;

namespace FCE.Windows.Core
{
    [FceItem(Name = "Service")]
    public class Service : ResourceBase
    {
        public override ResourceState Test(ConfigItem data)
        {
            var serviceName = data.Properties.Get("name");
            var serviceState = data.Properties.Get("state");
            var serviceStartType = data.Properties.Get("starttype");

            var result = PowerShellHelper.ExtractJson<Dictionary<string, string>>(PowerShellHelper.Run(new[]
            {
                "Write-Host '---start json---'",
                $"Get-Service -Name '{serviceName}' | Select-Object -Property Status, Name, StartType |  ConvertTo-Json",
                "Write-Host '---end json---'"
            }));

            if (serviceState != default(string))
            {
                switch (serviceState)
                {
                    case "start":
                        if (result["Status"] != "4")
                            return ResourceState.NotConfigured;
                        break;
                    case "stop":
                        if (result["Status"] != "1")
                            return ResourceState.NotConfigured;
                        break;
                    default:
                        throw new ResourceException("Invalid service state! Expect start or stop");
                }
            }

            if (serviceStartType != default(string))
            {
                switch (serviceStartType)
                {
                    case "automatic":
                        if (result["StartType"] != "2")
                            return ResourceState.NotConfigured;
                        break;
                    case "disabled":
                        if (result["StartType"] != "4")
                            return ResourceState.NotConfigured;
                        break;
                    case "manual":
                        if (result["StartType"] != "3")
                            return ResourceState.NotConfigured;
                        break;
                    default:
                        throw new ResourceException("Invalid startup type! Expect start or stop");
                }
            }

            return ResourceState.Configured;
        }

        public override ResourceState Apply(ConfigItem data)
        {
            var serviceName = data.Properties.Get("name");
            var serviceState = data.Properties.Get("state");
            var serviceStartType = data.Properties.Get("starttype");

            if (serviceState != default(string))
            {
                switch (serviceState)
                {
                    case "start":
                        PowerShellHelper.Run($"Start-Service '{serviceName}'");
                        break;
                    case "stop":
                        PowerShellHelper.Run($"Stop-Service '{serviceName}'");
                        break;
                }
            }

            if (serviceStartType != default(string))
            {
                switch (serviceStartType)
                {
                    case "automatic":
                        PowerShellHelper.Run($"Set-Service -Name '{serviceName}' -StartupType Automatic");
                        break;
                    case "disabled":
                        PowerShellHelper.Run($"Set-Service -Name '{serviceName}' -StartupType Disabled");
                        break;
                    case "manual":
                        PowerShellHelper.Run($"Set-Service -Name '{serviceName}' -StartupType Manual");
                        break;
                        
                }
            }

            return ResourceState.Configured;
        }
    }
}
