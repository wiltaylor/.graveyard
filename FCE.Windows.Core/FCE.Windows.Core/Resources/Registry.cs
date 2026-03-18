using System.Collections.Generic;
using System.Linq;
using FCE.Windows.Core.Helpers;
using FlexibleConfigEngine.Core.Exceptions;
using FlexibleConfigEngine.Core.Graph;
using FlexibleConfigEngine.Core.Helper;
using FlexibleConfigEngine.Core.IOC;
using FlexibleConfigEngine.Core.Resource;

namespace FCE.Windows.Core.Resources
{
    [FceItem(Name="Registry")]
    public class Registry : ResourceBase
    {
        public override ResourceState Test(ConfigItem data)
        {
            var regList = GetValue(data);

            var script = new List<string>();

            foreach (var entry in regList)
            {
                var opstring = entry.Exist ? "-eq" : "-ne";

                script.Add(entry.PropertyName != default(string)
                    ? $"(Get-ItemProperty -Path \"{entry.Hive}:\\{entry.KeyName}\" -Name '{entry.PropertyName}').\"{entry.PropertyName}\" {opstring} \"{entry.PropertyValue}\""
                    : $"(Test-Path '{entry.Hive}:\\{entry.KeyName}') {opstring} $true");
            }

            return PowerShellHelper.Run(script.ToArray()).Contains("False") ? ResourceState.NotConfigured : ResourceState.Configured;
        }

        private static IEnumerable<RegInfo> GetValue(IRestrictedConfigItem data)
        {
            var validhives = new[] { "hklm", "hkcu" };
            var globalhive = data.Properties.Get("hive");
            var globalkey = data.Properties.Get("key");

            var regList = new List<RegInfo>();

            foreach (var row in data.RowData)
            {
                var entry = new RegInfo
                {
                    KeyName = row.Get("key"),
                    PropertyName = row.Get("name"),
                    PropertyValue = row.Get("value"),
                    Exist = row.Get("ensure")?.ToLower() == "present",
                    Hive = row.Get("hive"),
                    Type = row.Get("type")
                };

                if (entry.Hive == default(string))
                    entry.Hive = globalhive;

                if (entry.KeyName == default(string))
                    entry.KeyName = globalkey;

                if (entry.KeyName == default(string))
                    throw new ResourceException("Missing key!");

                if (!validhives.Contains(entry.Hive))
                    throw new ResourceException("Expect hive to be hklm or hkcu!");

                regList.Add(entry);
            }

            return regList;
        }

        public override ResourceState Apply(ConfigItem data)
        {
            var regList = GetValue(data);

            var script = new List<string>();

            foreach (var entry in regList)
            {

                if (entry.Exist)
                {
                    script.Add($"&reg add {entry.Hive}\\{entry.KeyName} /f");

                    if (entry.KeyName == default(string)) continue;


                    //Using reg.exe instead of powershell cmdlets because it is better at creating nested keys.
                    script.Add($"&reg add {entry.Hive}\\{entry.KeyName} /v \"{entry.PropertyName}\" /t {entry.Type} /d \"{entry.PropertyValue}\" /f");
                }
                else
                {
                    script.Add(entry.PropertyName == default(string)
                        ? $"Remove-Item -Path \"{entry.Hive}:\\{entry.KeyName}\" -Force -Recurse"
                        : $"Remove-ItemProperty -Path \"{entry.Hive}:\\{entry.KeyName}\" -Name '{entry.PropertyName}'");
                }
            }

            PowerShellHelper.Run(script.ToArray());

            return ResourceState.Configured;
        }


        private struct RegInfo
        {
            public string KeyName;
            public string PropertyName;
            public string PropertyValue;
            public bool Exist;
            public string Hive;
            public string Type;
        }
    }
}
