using FlexibleConfigEngine.Core.Resource;

namespace FlexibleConfigEngine.Core.Config
{
    public class ConfigurationResult
    {
        public string Name { get; set; }
        public string Resource { get; set; }
        public ResourceState State { get; set; }
        public bool RanResource { get; set; }
    }
}
