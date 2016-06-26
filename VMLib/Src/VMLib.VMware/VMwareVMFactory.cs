using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using SystemWrapper.IO;
using VMLib.Exceptions;
using VMLib.IOC;
using VMLib.VMware.VIXItems;

namespace VMLib.VMware
{
    public class VMwareVMFactory : IVMFactory
    {
        private const string HypervisorName = "VMwareWorkstation";

        private readonly IFileWrap _file;
        private readonly IVix _vix;
        private IHypervisorConnectionInfo _hypervisorConnectionInfo;

        public VMwareVMFactory(IFileWrap file, IVix vix)
        {
            _file = file;
            _vix = vix;
        }

        public IVirtualMachine Create(IVMCreationInfo info)
        {
            if(info == null)
                throw new ArgumentNullException(nameof(info));
            if(string.IsNullOrEmpty(info.Path))
                throw new InvalidVMCreationInfoException("Invalid path!");
            if(info.CPU < 1)
                throw new InvalidVMCreationInfoException("CPU must be greater than 0");
            if(info.Cores < 1)
                throw new InvalidVMCreationInfoException("Cores must be greater than 0");
            if(info.Memory < 1)
                throw new InvalidVMCreationInfoException("Memory must be greater than 0");
            if(string.IsNullOrEmpty(info.Name))
                throw new InvalidVMCreationInfoException("Must not have an empty or null name!");

            var vmxcpu = info.CPU * info.Cores; 
            var vmxcore = info.Cores;

            var vmx = ServiceDiscovery.Instance.Resolve<IVMXHelper>(HypervisorName);
            //VMX Header info. Must be at the top.
            vmx.WriteVMX(".encoding", "windows-1252");
            vmx.WriteVMX("config.version", "8");
            vmx.WriteVMX("virtualHW.version", "12");

            vmx.WriteVMX("numvcpus", vmxcpu.ToString());
            vmx.WriteVMX("cpuid.coresPerSocket", vmxcore.ToString());
            vmx.WriteVMX("memsize", info.Memory.ToString());
            vmx.WriteVMX("displayName", info.Name);
            vmx.WriteVMX("guestOS", GetGuestOSString(info.GuestOS));

            foreach(var network in info.Networks)
                vmx.WriteNetwork(network);

            foreach(var disk in info.Disks)
                vmx.WriteDisk(disk);

            foreach(var setting in info.CustomSettings.Keys)
                vmx.WriteVMX(setting, info.CustomSettings[setting]);

            var vmxdata = vmx.ToArray();

            _file.WriteAllLines(info.Path, vmxdata);

            return Open(info.Path);
        }

        private string GetGuestOSString(GuestOS os)
        {
            switch (os)
            {
                case GuestOS.WindowsXPx86:
                    return "winxppro";
                case GuestOS.WindowsXPx64:
                    return "winxppro-64";
                case GuestOS.Windows2003x86:
                    return "winnetenterprise";
                case GuestOS.Windows2003x64:
                    return "winnetenterprise-64";
                case GuestOS.WindowsVistax86:
                    return "winvista";
                case GuestOS.WindowsVistax64:
                    return "winvista-64";
                case GuestOS.Windows2008x86:
                    return "longhorn";
                case GuestOS.Windows2008x64:
                    return "longhorn-64";
                case GuestOS.Windows7x86:
                    return "windows7";
                case GuestOS.Windows7x64:
                    return "windows7-64";
                case GuestOS.Windows2008R2x64:
                    return "windows7srv-64";
                case GuestOS.Windows8x64:
                    return "windows8";
                case GuestOS.Windows8x86:
                    return "windows8-64";
                case GuestOS.Windows2012x64:
                    return "windows8srv-64";
                case GuestOS.Windows81x86:
                    return "windows8";
                case GuestOS.Windows81x64:
                    return "windows8-64";
                case GuestOS.Windows2012R2x64:
                    return "windows8srv-64";
                case GuestOS.Windows10x86:
                    return "windows9";
                case GuestOS.Windows10x64:
                    return "windows9-64";
                case GuestOS.Windows2016x64:
                    return "windows9-64";
            }

            throw new ArgumentException(nameof(os));
        }

        public IVirtualMachine Open(string path)
        {
            if(path == null)
                throw new ArgumentNullException(nameof(path));

            if(!_file.Exists(path))
                throw new FileNotFoundException($"Can't find VM at {path}");

            return new VMwareVirtualMachine(path, _vix, ServiceDiscovery.Instance.Resolve<IVMXHelper>(HypervisorName), _hypervisorConnectionInfo);
        }

        public IEnumerable<IVirtualMachine> GetAllRunning()
        {
            return from v in _vix.GetAllRunning()
                select Open(v);
        }

        public void SetConnectionInfo(IHypervisorConnectionInfo info)
        {
            _hypervisorConnectionInfo = info;
        }
    }
}
