using System.Collections.Generic;

namespace VMLib.VM
{
    public class VMHardDisk : IVMDisk
    {
        public VMDiskType Type => VMDiskType.HardDisk;
        public string Path { get; set; }
        public IDictionary<string, string> CustomSettings { get; set; }
    }
}
