using System;

namespace VMLib.Exceptions
{
    public class UnsupportedVMFeature : Exception
    {
        public UnsupportedVMFeature(string message) : base(message)
        {
            
        }
    }
}
