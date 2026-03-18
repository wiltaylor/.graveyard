using System;
using SystemWrapper.Microsoft.Win32;

namespace VMLib.VirtualBox
{
    public class VirtualBoxHypervisorInfo : HypervisorInfoBase
    {
        public override string Name => "VirtualBox";
        private readonly IRegistryWrap _reg;

        public VirtualBoxHypervisorInfo(IRegistryWrap reg)
        {
            _reg = reg;
        }

        public override IHypervisorConnectionInfo CreateConnectionInfo()
        {
            var info = new VirtualBoxHypervisorConnectionInfo();

            var key = default(IRegistryKeyWrap);
            var path = "";

            try
            {
                key =  _reg.LocalMachine.OpenSubKey("SOFTWARE\\Oracle\\VirtualBox");
                path = key.GetValue("InstallDir").ToString();
            }
            catch { /* do nothing */}


            info.Properties.Add("VirtualBoxPath", path);

            return info;
        }
    }
}
