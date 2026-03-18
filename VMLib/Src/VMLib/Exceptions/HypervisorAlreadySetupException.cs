using System;

namespace VMLib.Exceptions
{
    public class HypervisorAlreadySetupException : Exception
    {
        public HypervisorAlreadySetupException(string message) : base(message)
        {
            
        }
    }
}
