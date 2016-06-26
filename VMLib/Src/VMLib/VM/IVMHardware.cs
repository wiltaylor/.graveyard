using System.Collections.Generic;

namespace VMLib
{
    public interface IVMHardware
    {
        IDictionary<string, string> CustomSettings { get; set; }
    }
}