using System.Collections.Generic;

namespace FlexibleConfigEngine.Core.Graph
{
    public interface IGraphManager
    {
        void Validate();
        IEnumerable<Gather> Gathers { get; }
        IEnumerable<ConfigItem> Config { get; }
        void AddGather(Gather gather);
        void AddConfig(ConfigItem config);
    }
}
