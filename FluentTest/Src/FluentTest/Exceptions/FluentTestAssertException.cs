using System;

namespace FluentTest.Exceptions
{
    public class FluentTestAssertException : Exception
    {
        public FluentTestAssertException(string message) : base(message)
        {
            
        }

        public FluentTestAssertException(string message, Exception e) : base(message, e)
        {

        }
    }
}
