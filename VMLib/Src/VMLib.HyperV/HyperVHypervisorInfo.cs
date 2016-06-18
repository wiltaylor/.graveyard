using System;
using System.Xml.Schema;

namespace VMLib.HyperV
{
    public class HyperVHypervisorInfo : HypervisorInfoBase
    {
        public override string Name => "HyperV";

        public override IHypervisorConnectionInfo CreateConnectionInfo()
        {
            var info = new HyperVHypervisorConnectionInfo();
            info.Properties.Add("Host", "localhost");
            return info;
        }

        public override IHypervisor CreateHypervisor(IHypervisorConnectionInfo info)
        {
            throw new NotImplementedException();
        }
    }
}
