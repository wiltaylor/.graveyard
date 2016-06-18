using System.Collections.Generic;

namespace VMLib.VMware
{
    public interface IVMXHelper
    {
        void WriteVMX(string setting, string value);
        void WriteNetwork(IVMNetwork network);
        void WriteDisks(IVMDisk disk);
        string[] ToArray();
    }
}
