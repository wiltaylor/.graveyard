using System.Collections.Generic;

namespace FlexibleConfigEngine.Core.Gather
{
    public interface IGatherDriver
    {
        void Run();
        void SetProperties(Dictionary<string,string> properties);
    }
}
