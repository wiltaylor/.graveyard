using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace VMLib.Utility
{
    public interface IEnvironmentHelper
    {
        string GetGUID();
        string TempDir { get; }
    }

    class EnvironmentHelper : IEnvironmentHelper
    {
        public string GetGUID()
        {
            return Guid.NewGuid().ToString();
        }

        public string TempDir => Environment.GetEnvironmentVariable("Temp");
    }
}
