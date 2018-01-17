using System.Collections.Generic;
using System.Linq;
using FlexibleConfigEngine.Core.Exceptions;

namespace FlexibleConfigEngine.Core.Graph
{
    public class GraphManager : IGraphManager
    {
        private readonly List<Gather> _gathers = new List<Gather>();
        private readonly List<ConfigItem> _configs = new List<ConfigItem>();

        public void Validate()
        {
            if(Config.Any(c1 => Config.Any(c2 => c1 != c2 && c1.Name == c2.Name)))
                throw new GraphValidationException("You can't use the same name between multiple items!");
        }

        public IEnumerable<Gather> Gathers => _gathers.AsReadOnly();
        public IEnumerable<ConfigItem> Config => _configs.AsReadOnly();

        public void AddGather(Gather gather)
        {
            _gathers.Add(gather);
        }

        public void AddConfig(ConfigItem config)
        {
            _configs.Add(config);
        }
    }
}
