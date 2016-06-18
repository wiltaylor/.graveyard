using System.Collections.Generic;

namespace VMLib.VMware
{
    public interface IVix
    {
        IEnumerable<string> GetAllRunning();
    }
}
