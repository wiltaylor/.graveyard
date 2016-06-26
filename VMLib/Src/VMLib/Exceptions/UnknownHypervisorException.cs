using System;

namespace VMLib.Exceptions
{
    public class UnknownHypervisorException : Exception
    {
        public UnknownHypervisorException(string message) : base(message)
        {
            
        }
    }
}
