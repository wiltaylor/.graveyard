using System;

namespace FlexibleConfigEngine.Core.Exceptions
{
    public class ResourceException : Exception
    {
        public ResourceException(string message) : base(message)
        {
            
        }

        public ResourceException(string message, Exception e) : base(message, e)
        {

        }
    }
}
