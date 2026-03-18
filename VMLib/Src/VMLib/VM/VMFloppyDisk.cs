using System.Collections.Generic;

namespace VMLib.VM
{
    public class VMFloppyDisk : IVMDisk
    {
        public VMDiskType Type => VMDiskType.Floppy;
        public string Path { get; set; }
        public IDictionary<string, string> CustomSettings { get; set; }
    }
}