using System.Collections.Generic;

namespace VMLib.VMware
{
    public class VMwareConnectionInfo : IHypervisorConnectionInfo
    {
        public Dictionary<string, string> Properties { get; set; }

        public VMwareConnectionInfo()
        {
            Properties = new Dictionary<string, string>();
        }
    }
}
