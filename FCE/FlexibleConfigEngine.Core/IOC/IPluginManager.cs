using FlexibleConfigEngine.Core.Gather;
using FlexibleConfigEngine.Core.Resource;

namespace FlexibleConfigEngine.Core.IOC
{
    public interface IPluginManager
    {
        IGatherDriver GetGather(string name);
        IResourceDriver GetResource(string name);
    }
}
