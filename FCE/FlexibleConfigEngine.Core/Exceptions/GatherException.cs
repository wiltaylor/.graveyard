using System;
using System.Collections.Generic;
using System.Text;

namespace FlexibleConfigEngine.Core.Exceptions
{
    public class GatherException : Exception
    {
        public GatherException(string message) : base(message)
        {
            
        }

        public GatherException(string message, Exception e) : base(message, e)
        {
            
        }
    }
}
