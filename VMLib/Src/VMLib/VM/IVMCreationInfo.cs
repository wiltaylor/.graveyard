using System.Collections.Generic;

namespace VMLib
{
    public interface IVMCreationInfo
    {
        string Path { get; set; }
        string Name { get; set; }
        int Memory { get; set; }
        int CPU { get; set; }
        int Cores { get; set; }
        IDictionary<string, string> CustomSettings { get; set; }
        IList<IVMDisk> Disks { get; set; }
        IList<IVMNetwork> Networks { get; set; }
        IList<IVMHardware> Hardware { get; set; }
    }
}