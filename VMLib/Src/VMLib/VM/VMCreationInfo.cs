using System.Collections.Generic;

namespace VMLib.VM
{
    public class VMCreationInfo : IVMCreationInfo
    {
        public string Path { get; set; }
        public string Name { get; set; }
        public GuestOS GuestOS { get; set; }
        public int Memory { get; set; }
        public int CPU { get; set; }
        public int Cores { get; set; }
        public IDictionary<string, string> CustomSettings { get; set; }
        public IList<IVMDisk> Disks { get; set; }
        public IList<IVMNetwork> Networks { get; set; }
    }
}