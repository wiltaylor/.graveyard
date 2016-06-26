using System.Collections.Generic;

namespace VMLib
{
    // ReSharper disable InconsistentNaming
    public enum GuestOS
    {
        WindowsXPx86,
        WindowsXPx64,
        Windows2003x86,
        Windows2003x64,
        WindowsVistax86,
        WindowsVistax64,
        Windows2008x86,
        Windows2008x64,
        Windows7x86,
        Windows7x64,
        Windows2008R2x64,
        Windows8x86,
        Windows8x64,
        Windows2012x64,
        Windows81x86,
        Windows81x64,
        Windows2012R2x64,
        Windows10x86,
        Windows10x64,
        Windows2016x64,
    }
    // ReSharper restore InconsistentNaming

    public interface IVMCreationInfo
    {
        string Path { get; set; }
        string Name { get; set; }
        GuestOS GuestOS { get; set; }
        int Memory { get; set; }
        int CPU { get; set; }
        int Cores { get; set; }
        IDictionary<string, string> CustomSettings { get; set; }
        IList<IVMDisk> Disks { get; set; }
        IList<IVMNetwork> Networks { get; set; }
    }
}