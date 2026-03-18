using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace VMLib.Exceptions
{
    public class VMPoweredOffException : Exception
    {
        public VMPoweredOffException(string message) : base(message)
        {
            
        }
    }
}
