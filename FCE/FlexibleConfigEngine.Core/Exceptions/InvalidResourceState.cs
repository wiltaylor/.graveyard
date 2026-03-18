using System;

namespace FlexibleConfigEngine.Core.Exceptions
{
    public class InvalidResourceState : Exception
    {
        public InvalidResourceState(string message): base(message)
        {
            
        }
    }
}
