using System;
using System.Collections.Generic;
using System.Text;

namespace EasyNuGet
{
    public class EasyNuGetException : Exception
    {

        public EasyNuGetException(string message) : base(message)
        {
            
        }

        public EasyNuGetException(string message, Exception inner) : base(message, inner)
        {
            
        }
    }
}
