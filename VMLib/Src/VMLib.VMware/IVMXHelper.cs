using System.Collections.Generic;

namespace VMLib.VMware
{
    public interface IVMXHelper
    {
        void WriteVMX(string setting, string value);
        void WriteNetwork(IVMNetwork network);
        void WriteDisk(IVMDisk disk);
        string[] ToArray();
        string ReadVMX(string setting);
        IEnumerable<IVMNetwork> ReadNetwork();
        void RemoveNetwork(IVMNetwork network);
        IEnumerable<IVMDisk> ReadDisk();
        void RemoveDisk(IVMDisk disk);
    }
}
