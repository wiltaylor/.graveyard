using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace VMLib.VMware.Exceptions
{
    public class VixException : Exception
    {
        public VixException(string message, ulong errorcode) : base(message)
        {
            
        }
    }
}
