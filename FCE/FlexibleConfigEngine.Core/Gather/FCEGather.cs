using System.Collections.Generic;
using System.Diagnostics;
using System.Reflection;
using FlexibleConfigEngine.Core.IOC;

namespace FlexibleConfigEngine.Core.Gather
{
    [FceItem(Name="FCE")]
    public class FceGather : IGatherDriver
    {
        private Dictionary<string, string> _properties;
        private readonly IDataStore _data;

        public FceGather(IDataStore data)
        {
            _data = data;
        }

        public void Run()
        {
            var versioninfo = FileVersionInfo.GetVersionInfo(Assembly.GetEntryAssembly().Location);

            _data.Write("FCE", new Dictionary<string, string>
            {
                ["FileVersion"] = versioninfo.FileVersion,
                ["ProductVersion"] = versioninfo.ProductVersion,
                ["TestValue"] = "Test"
            },true);
        }

        public void SetProperties(Dictionary<string, string> properties)
        {
            _properties = properties;
        }
    }
}
