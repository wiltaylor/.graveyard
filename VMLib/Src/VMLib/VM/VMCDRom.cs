using System.Collections.Generic;

namespace VMLib.VM
{
    public class VMCDRom : IVMDisk
    {
        public VMDiskType Type => VMDiskType.CDRom;
        public string Path { get; set; }
        public IDictionary<string, string> CustomSettings { get; set; }
    }
}
