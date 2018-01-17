using System;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using EasyRoslynScript;

namespace FlexibleConfigEngine.Core.Script
{
    public class ScriptBootStrap : IScriptPreCompileHandler
    {
        public string Process(string script, string folder)
        {
            return script;
        }

        public IEnumerable<Assembly> References => AppDomain.CurrentDomain.GetAssemblies()
            .Where(a => a.FullName.ToLower().Contains("flexibleconfigengine"));

        public IEnumerable<string> Imports => new[]
            {"FlexibleConfigEngine.Core.Script.Fluent", "FlexibleConfigEngine.Core", "FlexibleConfigEngine.Core.Graph"};

        public int Priority => 1;
    }
}
