using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace VMLib.Exceptions
{
    public class InvalidVMCreationInfoException : Exception
    {
        public InvalidVMCreationInfoException(string message) : base(message)
        {
            
        }
    }
}
