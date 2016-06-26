using System;

namespace VMLib.Exceptions
{
    public class VMPoweredOnException : Exception
    {
        public VMPoweredOnException(string message) : base(message)
        {
            
        }
    }
}
