using FlexibleConfigEngine.Core.Graph;
using FlexibleConfigEngine.Core.IOC;

namespace FlexibleConfigEngine.Core.Gather
{
    public class GatherManager : IGatherManager
    {
        private readonly IGraphManager _graphManager;
        private readonly IPluginManager _pluginManager;

        public GatherManager(IGraphManager graphManager, IPluginManager pluginManager)
        {
            _graphManager = graphManager;
            _pluginManager = pluginManager;
        }

        public void Run()
        {
            foreach (var g in _graphManager.Gathers)
            {
                var driver = _pluginManager.GetGather(g.Name);
                driver.SetProperties(g.Properties);
                driver.Run();
            }

        }
    }
}
