using System;
using System.Collections.Generic;
using EasyRoslynScript.NuGet;

namespace FlexibleConfigEngine.Core.Script
{
    public class NuGetScriptSettings : INuGetScriptSettings
    {
        public string PackageDir => Environment.CurrentDirectory + "\\Packages";
        public IEnumerable<string> SupportedPlatforms => new[] { "netstandard2.0", "netstandard1.6", "netstandard1.5",
            "netstandard1.4", "netstandard1.3", "netstandard1.2", "netstandard1.1", "netstandard1.0", "netcoreapp1.0",
            "netcoreapp1.1", "netcoreapp2.0" };

        public string DefaultRepository => "https://www.myget.org/F/fcepacks/api/v3/index.json";
    }
}
