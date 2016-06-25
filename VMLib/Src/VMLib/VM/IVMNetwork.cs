using System.Collections.Generic;

namespace VMLib
{
    public enum VMNetworkType
    {
        Bridged,
        NAT,
        HostOnly,
        Isolated
    }

    public interface IVMNetwork
    {
        VMNetworkType Type { get; set; }
        string MACAddress { get; set; }

        string IsolatedNetworkName { get; set; }

        IDictionary<string, string> CustomSettings { get; set; }
    }
}