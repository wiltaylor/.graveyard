using System;

namespace FluentTest.Exceptions
{
    public class FluentTestAssertException : Exception
    {
        public FluentTestAssertException(string message) : base(message)
        {
            
        }
    }
}
