using System.Collections;
using System.Collections.Generic;

namespace VMLib
{
    public enum VMDiskType
    {
        Floppy,
        HardDisk,
        CDRom
    }

    public interface IVMDisk
    {
        VMDiskType Type { get; }
        string Path { get; set; }
        IDictionary<string, string> CustomSettings { get; set; }


    }
}