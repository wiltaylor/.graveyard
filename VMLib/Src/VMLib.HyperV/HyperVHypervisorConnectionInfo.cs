using System.Collections.Generic;

namespace VMLib.HyperV
{
    public class HyperVHypervisorConnectionInfo : IHypervisorConnectionInfo
    {
        public Dictionary<string, string> Properties { get; set; }

        public HyperVHypervisorConnectionInfo()
        {
            Properties = new Dictionary<string, string>();
        }
    }
}
