using System.Collections.Generic;
using System.Security.Cryptography.X509Certificates;

namespace VMLib
{
    public interface IHypervisorConnectionInfo
    {
        Dictionary<string, string> Properties { get; set; }
    }
}
