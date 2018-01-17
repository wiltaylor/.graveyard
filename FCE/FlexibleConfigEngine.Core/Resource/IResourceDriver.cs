using FlexibleConfigEngine.Core.Graph;

namespace FlexibleConfigEngine.Core.Resource
{
    public enum ResourceState
    {
        Configured,
        NotConfigured,
        NeedReboot
    }

    public interface IResourceDriver
    {
        void GetData(ConfigItem data);
        ResourceState Test();
        ResourceState Apply();
    }
}
