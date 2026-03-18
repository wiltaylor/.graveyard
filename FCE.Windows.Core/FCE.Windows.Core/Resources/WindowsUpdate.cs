using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Reflection;
using FCE.Windows.Core.Helpers;
using FlexibleConfigEngine.Core.Graph;
using FlexibleConfigEngine.Core.Helper;
using FlexibleConfigEngine.Core.IOC;
using FlexibleConfigEngine.Core.Resource;

namespace FCE.Windows.Core.Resources
{
    [FceItem(Name="WindowsUpdate")]
    public class WindowsUpdate : ResourceBase
    {
        public override ResourceState Test(ConfigItem data)
        {
            var scriptfile = Path.GetFullPath($"{Path.GetDirectoryName(Assembly.GetExecutingAssembly().Location)}\\..\\..\\content\\Patch.ps1");

            if (data.RowData.Count == 0)
            {
                data.RowData.Add(new Dictionary<string, string>
                {
                    ["action"] = "skip",
                    ["patch"] = "Windows Defender"
                });

                data.RowData.Add(new Dictionary<string, string>
                {
                    ["action"] = "skip",
                    ["patch"] = "language pack"
                });
            }
            
            var script = new List<string>
            {
                "$skip = @("
            };

            script.AddRange(from row in data.RowData where row.Get("action") == "skip" select $"\"*{row.Get("patch")}*\"");

            script.Add(")");

            script.Add($"&'{scriptfile}' -ignorePatches $skip -Test");

            
            var result = PowerShellHelper.Run(script.ToArray());

            return result.Contains("###PATCH FINISHED###") ? ResourceState.Configured : ResourceState.NotConfigured;
        }

        public override ResourceState Apply(ConfigItem data)
        {
            var scriptFolder = Path.GetFullPath($"{Path.GetDirectoryName(Assembly.GetExecutingAssembly().Location)}\\..\\..\\content\\Patch.ps1");

            if (data.RowData.Count == 0)
            {
                data.RowData.Add(new Dictionary<string, string>
                {
                    ["action"] = "skip",
                    ["patch"] = "Windows Defender"
                });

                data.RowData.Add(new Dictionary<string, string>
                {
                    ["action"] = "skip",
                    ["patch"] = "language pack"
                });
            }

            var script = new List<string>
            {
                "$skip = @("
            };

            script.AddRange(from row in data.RowData where row.Get("action") == "skip" select $"\"*{row.Get("patch")}*\"");

            script.Add(")");

            script.Add($"&'{scriptFolder}' -ignorePatches $skip");
            
            var result = PowerShellHelper.Run(script.ToArray());

            return result.Contains("###REBOOT###")? ResourceState.NeedReboot : ResourceState.Configured;
        }
    }
}
