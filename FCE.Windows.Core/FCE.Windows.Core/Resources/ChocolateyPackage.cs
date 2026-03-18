using System;
using FCE.Windows.Core.Helpers;
using FlexibleConfigEngine.Core.Exceptions;
using FlexibleConfigEngine.Core.Graph;
using FlexibleConfigEngine.Core.Helper;
using FlexibleConfigEngine.Core.IOC;
using FlexibleConfigEngine.Core.Resource;

namespace FCE.Windows.Core.Resources
{
    [FceItem(Name= "ChocolateyPackage")]
    public class ChocolateyPackage : ResourceBase
    {
        public override ResourceState Test(ConfigItem data)
        {
            var packageName = data.Properties.Get("package");
            var version = data.Properties.Get("version");

            if (!PowerShellHelper.Run("(Get-Command choco).Name").ToLower().Contains("choco.exe"))
                return ResourceState.NotConfigured;


            if(packageName == default(string))
                throw new ResourceException("You need to pass in a package name!");

            var result = PowerShellHelper.Run($"choco list {packageName} --localonly -e");

            if (!result.ToLower().Contains(packageName.ToLower()))
                return ResourceState.NotConfigured;

            if (version != null && !result.Contains(version))
                return ResourceState.NotConfigured;

            return ResourceState.Configured;
        }

        public override ResourceState Apply(ConfigItem data)
        {
            var packageName = data.Properties.Get("package");
            var version = data.Properties.Get("version");
            var source = data.Properties.Get("source");

            if (!PowerShellHelper.Run("(Get-Command choco).Name").ToLower().Contains("choco.exe"))
            {
                PowerShellHelper.Run(
                    "Set-ExecutionPolicy Bypass -Scope Process -Force; iex ((New-Object System.Net.WebClient).DownloadString(\'https://chocolatey.org/install.ps1\'))");

                Environment.SetEnvironmentVariable("Path", Environment.GetEnvironmentVariable("Path") + ";C:\\ProgramData\\chocolatey\\bin;");
            }

            var command = $"choco install {packageName} -y ";

            if (version != default(string))
                command += $"--version {version} ";

            if (source != default(string))
                command += $"-s {source}";

            return PowerShellHelper.Run(new[]
            {
                command,
                "if($LASTEXITCODE -eq 3010) { Write-Host '###REBOOT###'}"
            }).Contains("###REBOOT###") ? ResourceState.NeedReboot : ResourceState.Configured;
        }
    }
}
