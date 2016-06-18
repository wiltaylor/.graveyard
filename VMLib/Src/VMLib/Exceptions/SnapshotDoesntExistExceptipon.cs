using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace VMLib.Exceptions
{
    public class SnapshotDoesntExistExceptipon : Exception
    {
        public SnapshotDoesntExistExceptipon(string message) : base(message)
        {
            
        }
    }
}
