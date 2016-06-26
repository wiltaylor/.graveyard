using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace VMLib.VirtualBox
{
    public class VirtualBoxHypervisorConnectionInfo : IHypervisorConnectionInfo
    {
        public Dictionary<string, string> Properties { get; set; }

        public VirtualBoxHypervisorConnectionInfo()
        {
            Properties = new Dictionary<string, string>();
        }
    }
}
