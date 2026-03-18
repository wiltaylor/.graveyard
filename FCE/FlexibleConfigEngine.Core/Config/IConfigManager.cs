using System.Collections.Generic;
using FlexibleConfigEngine.Core.Graph;

namespace FlexibleConfigEngine.Core.Config
{
    public interface IConfigManager
    {
        IEnumerable<ConfigItem> BuildRunList(Dictionary<string, string> settings);
        IEnumerable<ConfigurationResult> ApplyRunList(IEnumerable<ConfigItem> runlist);
        IEnumerable<ConfigurationResult> Test(IEnumerable<ConfigItem> runlist);
    }
}
