using System.Linq;
using FCE.Windows.Core.Helpers;
using FlexibleConfigEngine.Core.Exceptions;
using FlexibleConfigEngine.Core.Graph;
using FlexibleConfigEngine.Core.Helper;
using FlexibleConfigEngine.Core.IOC;
using FlexibleConfigEngine.Core.Resource;

namespace FCE.Windows.Core.Resources
{
    [FceItem(Name = "PowershellSecurity")]
    public class PowershellSecurity : ResourceBase
    {
        public override ResourceState Test(ConfigItem data)
        {
            var level = data.Properties.Get("policy")?.ToLower();
            var scope = data.Properties.Get("scope")?.ToLower() ?? "currentuser";
            var validLevels = new[] {"allsigned", "default", "remotesigned", "restricted", "undefined", "unrestricted"};
            var validScopes = new[] {"currentuser", "localmachine" };

            if(validLevels.All(v => v != level))
                throw new ResourceException("Invalid policy level for powershell policy!");

            if(validScopes.All(v => v != scope))
                throw new ResourceException("Invalid scope for powershell policy!");

            return PowerShellHelper.Run($"(Get-ExecutionPolicy -List | where Scope -eq {scope}).ExecutionPolicy").ToLower().Contains(level) ? 
                ResourceState.Configured : 
                ResourceState.NotConfigured;
        }

        public override ResourceState Apply(ConfigItem data)
        {
            var level = data.Properties.Get("policy")?.ToLower();
            var scope = data.Properties.Get("scope")?.ToLower() ?? "currentuser";

            PowerShellHelper.Run($"Set-ExecutionPolicy -ExecutionPolicy {level} -Scope {scope} -Force");

            return ResourceState.Configured;
        }
    }
}
