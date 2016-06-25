using System.Collections.Generic;

namespace VMLib.VM
{
    public class VMNetwork : IVMNetwork
    {
        public VMNetworkType Type { get; set; }
        public string MACAddress { get; set; }
        public string IsolatedNetworkName { get; set; }
        public IDictionary<string, string> CustomSettings { get; set; }
    }
}