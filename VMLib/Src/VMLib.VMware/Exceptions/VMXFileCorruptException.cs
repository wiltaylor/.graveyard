using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace VMLib.VMware.Exceptions
{
    public class VMXFileCorruptException : Exception
    {
        public VMXFileCorruptException(string message) : base(message)
        {
            
        }
    }
}
