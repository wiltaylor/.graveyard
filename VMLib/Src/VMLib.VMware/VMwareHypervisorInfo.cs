using SystemWrapper.Microsoft.Win32;

namespace VMLib.VMware
{
    public class VMwareHypervisorInfo : HypervisorInfoBase
    {
        public override string Name => "VMwareWorkstation";

        private readonly IRegistryWrap _reg;

        public VMwareHypervisorInfo(IRegistryWrap reg)
        {
            _reg = reg;
        }

        public override IHypervisorConnectionInfo CreateConnectionInfo()
        {
            var connectioninfo = new VMwareConnectionInfo();

            var path = "";

            foreach (var regkey in new[]
            {
                "SOFTWARE\\WOW6432Node\\VMware, Inc.\\VMware Workstation",
                "SOFTWARE\\VMware, Inc.\\VMware Workstation"
            })
            {
                try
                {
                    var key = _reg.LocalMachine.OpenSubKey(regkey);
                    path = key.GetValue("InstallPath").ToString();
                }
                catch { /* do nothing */ }

                //Fixes systemwrapper null behaviour.
                if (path == "Faked System.Object")
                    path = "";

                if (!string.IsNullOrEmpty(path))
                    break;
            }

            connectioninfo.Properties.Add("VMwareWorkstationPath", path);

            return connectioninfo;
        }

        public override IHypervisor CreateHypervisor(IHypervisorConnectionInfo info)
        {
            throw new System.NotImplementedException();
        }
    }
}
