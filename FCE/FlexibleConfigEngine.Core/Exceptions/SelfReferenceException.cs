using System;
using System.Collections.Generic;
using System.Text;

namespace FlexibleConfigEngine.Core.Exceptions
{
    public class SelfReferenceException : Exception
    {
        public SelfReferenceException(string name) : base($"Config Item {name} has a dependancy on itself.")
        {
            
        }
    }
}
